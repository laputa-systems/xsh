use core.lib.fat as vfat

test test_fsck_fat_cli_readonly_and_explicit_repair { |ctx|
  let root = test.temp_dir(ctx, name: "fsck-fat")?
  let image = fp"{root}/disk.img"
  let _ = vfat.format(image, 1474560, bits: 12)?
  let original = image.read_bytes()?
  image.write(bytes.concat([original.slice(0, length: 515), b"\xff\x0f", original.slice(517)]))
  let damaged = image.read_bytes()?
  let script = fp"{ctx.core_dir}/fsck.fat.xsh"
  let readonly = test.run_script(ctx, script.read_text()?, env: {XSH_MODULE_PATH: ctx.core_dir.display()}, args: ["-n", image.display()])?
  assert readonly.status == 1, readonly.stderr
  assert image.read_bytes()? == damaged
  let repaired = test.run_script(ctx, script.read_text()?, env: {XSH_MODULE_PATH: ctx.core_dir.display()}, args: ["-a", image.display()])?
  assert repaired.status == 1, repaired.stderr
  assert vfat.check(image)?.issues.is_empty()
  assert test.run_script(ctx, script.read_text()?, env: {XSH_MODULE_PATH: ctx.core_dir.display()}, args: ["-n", image.display()])?.status == 0
}

test test_fat_checker_truncates_loops_and_crosslinked_files { |ctx|
  let root = test.temp_dir(ctx, name: "fat-chains")?
  let image = fp"{root}/disk.img"
  let _ = vfat.format(image, 1474560, bits: 12)?
  let entry = bytes.concat([b"FILE    TXT\x20", bytes.zero(14)?, bytes.pack_le(2, 2)?, bytes.pack_le(1024, 4)?])
  let _ = bytes.write_at(image, 9728, entry)?
  let _ = bytes.write_at(image, 515, b"\x03\x20\x00")?
  let _ = bytes.write_at(image, 5123, b"\x03\x20\x00")?
  let loop_report = vfat.check(image)?
  assert ! loop_report.issues.is_empty()
  let fixed = vfat.check(image, repair: true)?
  assert fixed.unresolved == 0
  assert vfat.check(image)?.issues.is_empty()
  let second = bytes.concat([b"OTHER   TXT\x20", bytes.zero(14)?, bytes.pack_le(2, 2)?, bytes.pack_le(512, 4)?])
  let _ = bytes.write_at(image, 9760, second)?
  assert ! vfat.check(image)?.issues.is_empty()
  assert vfat.check(image, repair: true)?.unresolved == 0
  assert vfat.check(image)?.issues.is_empty()
}

test test_fat_checker_removes_orphan_long_filename_entries { |ctx|
  let root = test.temp_dir(ctx, name: "fat-lfn")?
  let image = fp"{root}/disk.img"
  let _ = vfat.format(image, 1474560, bits: 12)?
  let _ = bytes.write_at(image, 9728, bytes.concat([b"\x41", bytes.zero(10)?, b"\x0f", bytes.zero(20)?]))?
  assert ! vfat.check(image)?.issues.is_empty()
  assert vfat.check(image, repair: true)?.unresolved == 0
  assert vfat.check(image)?.issues.is_empty()
}

test test_fat_checker_refuses_fifo_without_waiting_for_writer { |ctx|
  let root = test.temp_dir(ctx, name: "fat-fifo")?
  let fifo = fp"{root}/pipe"
  fs.mkfifo(fifo, 0o600)
  assert vfat.check(fifo) is Err(_)
  assert vfat.inspect(fifo) is Err(_)
  assert vfat.label(fifo, label: "NO") is Err(_)
}

test test_fat_checker_clears_invalid_cluster_on_empty_file { |ctx|
  let root = test.temp_dir(ctx, name: "fat-empty-chain")?
  let image = fp"{root}/disk.img"
  let _ = vfat.format(image, 1474560, bits: 12)?
  let entry = bytes.concat([b"EMPTY   TXT\x20", bytes.zero(14)?, bytes.pack_le(65535, 2)?, bytes.zero(4)?])
  let _ = bytes.write_at(image, 9728, entry)?
  assert ! vfat.check(image)?.issues.is_empty()
  assert vfat.check(image, repair: true)?.unresolved == 0
  assert vfat.check(image)?.issues.is_empty()
}

test test_fat_checker_preserves_bad_cluster_marker_when_truncating_file { |ctx|
  let root = test.temp_dir(ctx, name: "fat-bad-chain")?
  let image = fp"{root}/disk.img"
  let _ = vfat.format(image, 1474560, bits: 12)?
  let entry = bytes.concat([b"BROKEN  TXT\x20", bytes.zero(14)?, bytes.pack_le(2, 2)?, bytes.pack_le(512, 4)?])
  let _ = bytes.write_at(image, 9728, entry)?
  let _ = bytes.write_at(image, 515, b"\xf7\x0f")?
  let _ = bytes.write_at(image, 5123, b"\xf7\x0f")?
  assert ! vfat.check(image)?.issues.is_empty()
  assert vfat.check(image, repair: true)?.unresolved == 0
  assert bytes.read_at(image, 515, 2)? == b"\xf7\x0f"
  assert bytes.read_at(image, 5123, 2)? == b"\xf7\x0f"
  assert vfat.check(image)?.issues.is_empty()
}
