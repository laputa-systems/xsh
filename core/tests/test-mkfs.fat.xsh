use core.lib.fat as vfat

test test_fat_geometry_labels_and_readonly_check { |ctx|
  let root = test.temp_dir(ctx, name: "fat")?
  let image = fp"{root}/disk.img"
  let layout = vfat.format(image, 1474560, bits: 12, label: "FIXTURE", serial: 1234)?
  assert layout.bits == 12
  assert layout.sector_size == 512
  assert vfat.inspect(image)?.label == "FIXTURE"
  assert vfat.inspect(image)?.serial == 1234
  let before = image.read_bytes()?
  assert vfat.check(image)?.issues.is_empty()
  assert image.read_bytes()? == before
  assert vfat.label(image, label: "NEWLABEL")? == "NEWLABEL"
  assert vfat.label(image)? == "NEWLABEL"
  assert vfat.check(image)?.issues.is_empty()
}

test test_fat_rejects_invalid_geometry_and_label { |ctx|
  let root = test.temp_dir(ctx, name: "fat-errors")?
  let image = fp"{root}/disk.img"
  assert vfat.format(image, 4096, bits: 32) is Err(_)
  assert vfat.format(image, 2119680, bits: 12, cluster: 1) is Err(_)
  assert vfat.format(image, 1474560, label: "ß") is Err(_)
  assert vfat.format(image, 1474560, sector: 1000) is Err(_)
  assert vfat.format(image, 1474560, cluster: 3) is Err(_)
  assert vfat.format(image, 1474560, label: "TOO-LONG-LABEL") is Err(_)
  assert ! image.exists()?
}

test test_fat_allocation_checker_repairs_lost_cluster { |ctx|
  let root = test.temp_dir(ctx, name: "fat-lost")?
  let image = fp"{root}/disk.img"
  let _ = vfat.format(image, 1474560, bits: 12)?
  let original = image.read_bytes()?
  # Cluster two occupies the low twelve bits at FAT offset three.
  let damaged = bytes.concat([original.slice(0, length: 515), b"\xff\x0f", original.slice(517)])
  image.write(damaged)
  assert ! vfat.check(image)?.issues.is_empty()
  assert image.read_bytes()? == damaged
  let repaired = vfat.check(image, repair: true)?
  assert repaired.repaired > 0
  assert repaired.unresolved == 0
  assert vfat.check(image)?.issues.is_empty()
}

test test_mkfs_fat_cli_reproducible_image { |ctx|
  let root = test.temp_dir(ctx, name: "mkfs-fat-cli")?
  let image = fp"{root}/image"
  let result = test.run_script(ctx, fp"{ctx.core_dir}/mkfs.fat.xsh".read_text()?, env: {XSH_MODULE_PATH: ctx.core_dir.display()}, args: ["-C", "--invariant", "-F12", "-n", "TEST", image.display(), "1440"])?
  assert result.status == 0, result.stderr
  assert vfat.inspect(image)?.label == "TEST"
  assert vfat.inspect(image)?.serial == 0
}

test test_fat_format_preserves_bytes_outside_volume { |ctx|
  let root = test.temp_dir(ctx, name: "fat-boundary")?
  let image = fp"{root}/disk.img"
  let _ = bytes.write_at(image, 2097148, b"TAIL", create: true)?
  let _ = vfat.format(image, 1474560, bits: 12)?
  assert image.metadata()?.size == 2097152
  assert bytes.read_at(image, 2097148, 4)? == b"TAIL"
  assert vfat.check(image)?.issues.is_empty()
}

test test_fat16_and_fat32_sparse_images { |ctx|
  let root = test.temp_dir(ctx, name: "fat-types")?
  let fat16 = fp"{root}/fat16.img"
  let layout16 = vfat.format(fat16, 16777216, bits: 16, label: "FAT16")?
  assert layout16.bits == 16
  assert vfat.check(fat16)?.issues.is_empty()
  let fat32 = fp"{root}/fat32.img"
  let layout32 = vfat.format(fat32, 67108864, bits: 32, label: "FAT32")?
  assert layout32.bits == 32
  assert vfat.check(fat32)?.issues.is_empty()
  assert vfat.label(fat32, label: "CHANGED")? == "CHANGED"
  assert vfat.check(fat32)?.issues.is_empty()
  let _ = bytes.write_at(fat32, 3072, b"\0")?
  assert ! vfat.check(fat32)?.issues.is_empty()
  assert vfat.check(fat32, repair: true)?.unresolved == 0
  assert vfat.check(fat32)?.issues.is_empty()
}


test test_fat_exclusive_creation_preserves_existing_image { |ctx|
  let root = test.temp_dir(ctx, name: "fat-exclusive")?
  let image = fp"{root}/disk.img"
  image.write("keep existing content")
  assert vfat.format(image, 1474560, bits: 12, create: true) is Err(_)
  assert image.read_text()? == "keep existing content"
}


test test_fat_automatic_type_selection { |ctx|
  let root = test.temp_dir(ctx, name: "fat-auto")?
  assert vfat.format(fp"{root}/small.img", 1474560)?.bits == 12
  assert vfat.format(fp"{root}/medium.img", 16777216)?.bits == 16
  assert vfat.format(fp"{root}/large.img", 536870912)?.bits == 32
}


test test_mkfs_fat_cli_invalid_values_leave_target_absent { |ctx|
  let root = test.temp_dir(ctx, name: "mkfs-fat-invalid")?
  let image = fp"{root}/image"
  let script = fp"{ctx.core_dir}/mkfs.fat.xsh".read_text()?
  assert test.run_script(ctx, script, env: {XSH_MODULE_PATH: ctx.core_dir.display()}, args: ["-C", image.display(), "invalid"])?.status == 1
  assert test.run_script(ctx, script, env: {XSH_MODULE_PATH: ctx.core_dir.display()}, args: ["-C", "-i", "invalid", image.display(), "1440"])?.status == 1
  assert ! image.exists()?
}

test test_fat_nondefault_sector_reserved_and_fat_counts { |ctx|
  let root = test.temp_dir(ctx, name: "fat-layout")?
  let image = fp"{root}/disk.img"
  let layout = vfat.format(image, 67108864, bits: 16, sector: 1024, cluster: 2, reserved: 8, fats: 1, label: "ALTERNATE", serial: 4294967295)?
  assert layout.sector_size == 1024
  assert layout.cluster_size == 2048
  assert vfat.inspect(image)?.label == "ALTERNATE"
  assert vfat.inspect(image)?.serial == 4294967295
  assert vfat.check(image)?.issues.is_empty()
}
