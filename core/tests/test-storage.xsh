use core.lib.storage

type ListedPartition = {start: Int, size: Int}
type ListedTable = {label: Str, partitions: List[ListedPartition]}
type ListedDisk = {partitiontable: ListedTable}

type StorageRun = {success: Bool, status: Int, stdout: Str, stderr: Str, stdout_bytes: Bytes, stderr_bytes: Bytes}

proc run_storage(ctx: TestContext, name: Str, args: List[Str], log: Path) -> StorageRun {
  test.linux_fake(ctx, {log: log})
  let source = fp"{ctx.core_dir}/{name}.xsh".read_text()?
  test.run_script(ctx, source, args, {XSH_MODULE_PATH: ctx.core_dir}, b"", name)?
}

test test_storage_blkid_uses_conventional_tags_and_native_probe { |ctx|
  let log = test.temp_file(ctx, name: "blkid-log", contents: b"")?
  let output = run_storage(ctx, "blkid", ["-s", "TYPE", "-o", "value", "/owned/image"], log)
  assert output.success, output.stderr
  assert output.stdout == "ext4\n"
  assert "\"op\":\"blkid\"" in log.read_text()?
}

test test_storage_help_has_no_native_effects { |ctx|
  let log = test.temp_file(ctx, name: "storage-help-log", contents: b"")?
  for name in ["lsblk", "blkid", "findmnt", "mount", "umount", "losetup", "blockdev", "wipefs", "partx", "partprobe", "fstrim", "fsfreeze", "sfdisk", "fdisk", "swapon", "swapoff", "mkswap"] {
    let output = run_storage(ctx, name, ["--help"], log)
    assert output.success, f"{name}: {output.stderr}"
    assert f"Usage: {name}" in output.stdout
  }
  assert log.read_text()? == ""
}

test test_storage_unknown_options_fail_before_mutation { |ctx|
  let log = test.temp_file(ctx, name: "storage-invalid-log", contents: b"")?
  for name in ["mount", "losetup", "mkswap", "swapon", "swapoff", "sfdisk", "wipefs"] {
    let output = run_storage(ctx, name, ["--not-supported", "/dev/example"], log)
    assert ! output.success
    assert "not supported" in output.stderr or "unrecognized option" in output.stderr
  }
  assert log.read_text()? == ""
}

test test_storage_mount_and_swap_translate_named_targets { |ctx|
  let log = test.temp_file(ctx, name: "storage-control-log", contents: b"")?
  let mounted = run_storage(ctx, "mount", ["-t", "tmpfs", "-o", "nosuid,nodev", "tmpfs", "/owned/mount"], log)
  assert mounted.success, mounted.stderr
  let swap = run_storage(ctx, "swapon", ["-p", "7", "/owned/swap"], log)
  assert swap.success, swap.stderr
  let output = log.read_text()?
  assert "\"op\":\"mount\"" in output
  assert "nosuid" in output and "nodev" in output
  assert "\"op\":\"swapon\"" in output
}

test test_storage_filesystem_type_filter_supports_exclusions {
  assert storage.type_matches("ext4", "ext4,xfs")
  assert ! storage.type_matches("tmpfs", "ext4,xfs")
  assert storage.type_matches("ext4", "notmpfs,proc")
  assert ! storage.type_matches("tmpfs", "notmpfs,proc")
  assert ! storage.type_matches("proc", "notmpfs,proc")
}

test test_storage_lsblk_rooted_json_preserves_partition_identity { |ctx|
  let dir = test.temp_dir(ctx, name: "lsblk-root")?
  let root = fs.open_root(dir)?
  defer root.close()
  root.mkdir(p"sys/class/block", parents: true)
  for name in ["disk", "disk1"] {
    root.mkdir(fp"sys/devices/disk/{if name == "disk" { "" } else { "disk1/" }}queue", parents: true)
    let at = if name == "disk" { "sys/devices/disk" } else { "sys/devices/disk/disk1" }
    root.write(fp"{at}/dev", if name == "disk" { "8:0\n" } else { "8:1\n" })
    root.write(fp"{at}/size", if name == "disk" { "100\n" } else { "50\n" })
    root.write(fp"{at}/ro", "0\n")
    root.write(fp"{at}/removable", "0\n")
    root.symlink(target: fp"../../devices/disk/{if name == "disk" { "" } else { "disk1" }}", path: fp"sys/class/block/{name}")
  }
  root.write(p"sys/devices/disk/disk1/partition", "1\n")
  root.mkdir(p"proc/self", parents: true)
  root.write(p"proc/self/mountinfo", "1 0 8:1 / /owned/mount rw - ext4 /dev/alias rw\n")
  let source = "use lib.storage\nproc main(...argv: List[Str]) { let root = fs.open_root(fp\"{argv[0]}\")?; defer root.close(); storage.lsblk_from_root(root, [\"-Jb\", \"-o\", \"NAME,SIZE,TYPE,MOUNTPOINT\"]) }"
  let output = test.run_script(ctx, source, [f"{dir}"], {XSH_MODULE_PATH: ctx.core_dir}, b"", "lsblk")?
  assert output.success, output.stderr
  assert "\"children\"" in output.stdout
  assert "\"size\":51200" in output.stdout
  assert "\"size\":25600" in output.stdout
  assert "\"mountpoint\":\"/owned/mount\"" in output.stdout
}

test test_storage_sfdisk_rejects_overlapping_input_before_write { |ctx|
  let log = test.temp_file(ctx, name: "sfdisk-overlap-log", contents: b"")?
  test.linux_fake(ctx, {log: log})
  let source = fp"{ctx.core_dir}/sfdisk.xsh".read_text()?
  let output = test.run_script(ctx, source, ["/owned/disk"], {XSH_MODULE_PATH: ctx.core_dir}, b"label: gpt\nstart=2048,size=4096\nstart=4096,size=4096\n", "sfdisk")?
  assert ! output.success
  assert "overlap" in output.stderr
  assert log.read_text()? == ""
}

test test_storage_sfdisk_writes_conventional_named_ranges { |ctx|
  let log = test.temp_file(ctx, name: "sfdisk-write-log", contents: b"")?
  test.linux_fake(ctx, {log: log})
  let source = fp"{ctx.core_dir}/sfdisk.xsh".read_text()?
  let output = test.run_script(ctx, source, ["/owned/disk"], {XSH_MODULE_PATH: ctx.core_dir}, b"label: gpt\nunit: sectors\nstart=2048,size=8M,type=L,name=\"root\"\n", "sfdisk")?
  assert output.success, output.stderr
  let recorded = log.read_text()?
  assert "\"op\":\"write_partition_table\"" in recorded
  let dry = test.run_script(ctx, source, ["--no-act", "/owned/disk"], {XSH_MODULE_PATH: ctx.core_dir}, b"label: gpt\nunit: sectors\nstart=2048,size=8M,type=L,name=\"root\"\n", "sfdisk")?
  assert dry.success, dry.stderr
  assert "root" in dry.stdout
  assert "16384" in dry.stdout
  assert log.read_text()? == recorded
}

test test_storage_findmnt_rooted_json_filters_type_and_source { |ctx|
  let dir = test.temp_dir(ctx, name: "findmnt-root")?
  let root = fs.open_root(dir)?
  defer root.close()
  root.mkdir(p"proc/self", parents: true)
  root.write(p"proc/self/mountinfo", "1 0 0:1 / / rw - rootfs rootfs rw\n2 1 8:1 / /owned/a\\040b rw - ext4 /dev/disk1 rw,errors=remount-ro\n3 1 0:3 / /proc rw - proc proc rw\n")
  let source = "use lib.storage\nproc main(...argv: List[Str]) { let root = fs.open_root(fp\"{argv[0]}\")?; defer root.close(); storage.findmnt_from_root(root, [\"-J\", \"-t\", \"ext4\", \"-S\", \"/dev/disk1\", \"-o\", \"TARGET,FSTYPE,MAJ:MIN\"]) }"
  let output = test.run_script(ctx, source, [f"{dir}"], {XSH_MODULE_PATH: ctx.core_dir}, b"", "findmnt")?
  assert output.success, output.stderr
  assert "\"filesystems\"" in output.stdout
  assert "\"target\":\"/owned/a b\"" in output.stdout
  assert "\"maj:min\":\"8:1\"" in output.stdout
  assert "rootfs" not in output.stdout and "proc" not in output.stdout
}

test test_storage_wipefs_selectively_erases_owned_signature_only { |ctx|
  let image = test.temp_file(ctx, name: "wipefs.img", contents: b"XFSBpayload-preserved")?
  let source = fp"{ctx.core_dir}/wipefs.xsh".read_text()?
  let listing = test.run_script(ctx, source, ["--json", f"{image}"], {XSH_MODULE_PATH: ctx.core_dir}, b"", "wipefs")?
  assert listing.success, listing.stderr
  assert "\"offset\": \"0x0\"" in listing.stdout
  assert "\"type\": \"xfs\"" in listing.stdout
  assert "\"uuid\": null" in listing.stdout
  let dry = test.run_script(ctx, source, ["--no-act", "--offset", "0x0", f"{image}"], {XSH_MODULE_PATH: ctx.core_dir}, b"", "wipefs")?
  assert dry.success, dry.stderr
  assert image.read_bytes()? == b"XFSBpayload-preserved"
  let erased = test.run_script(ctx, source, ["--offset", "0x0", f"{image}"], {XSH_MODULE_PATH: ctx.core_dir}, b"", "wipefs")?
  assert erased.success, erased.stderr
  assert image.read_bytes()? == b"\0\0\0\0payload-preserved"
}

# A FAT boot sector carries three magics: the FATnn type string, the jump byte
# at offset 0 and the 0x55aa trailer. Erasing only the type string leaves the
# trailer, which a probe then reports as a DOS partition table.
test test_storage_wipefs_erases_every_magic_of_a_fat_filesystem { |ctx|
  let image = test.temp_file(ctx, name: "wipefs-fat.img", contents: b"")?
  image.truncate(2097152)
  let made = run_real(ctx, "mkfs.fat", ["-F", "12", "-n", "TESTFAT", f"{image}"])
  assert made.success, made.stderr
  let listed = run_real(ctx, "wipefs", [f"{image}"])
  assert listed.success, listed.stderr
  let rows = [line.words() for line in listed.stdout.lines()]
  assert rows[0] == ["DEVICE", "OFFSET", "TYPE", "UUID", "LABEL"]
  assert [rows[at][1] for at in range(1, rows.len())] == ["0x36", "0x0", "0x1fe"]
  assert [rows[at][2] for at in range(1, rows.len())] == ["vfat", "vfat", "vfat"]
  assert rows[1][4] == "TESTFAT"

  let expected = test.temp_file(ctx, name: "wipefs-fat-expected.img", contents: image.read_bytes()?)?
  let _ = bytes.zero_at(expected, 54, 8)?
  let _ = bytes.zero_at(expected, 0, 1)?
  let _ = bytes.zero_at(expected, 510, 2)?
  let dry = run_real(ctx, "wipefs", ["-n", "-a", f"{image}"])
  assert dry.success, dry.stderr
  assert dry.stdout.lines().len() == 3
  assert image.read_bytes()? != expected.read_bytes()?

  let erased = run_real(ctx, "wipefs", ["-a", f"{image}"])
  assert erased.success, erased.stderr
  assert erased.stdout.lines() == [
    f"{image}: 8 bytes were erased at offset 0x00000036 (vfat): 46 41 54 31 32 20 20 20",
    f"{image}: 1 byte was erased at offset 0x00000000 (vfat): eb",
    f"{image}: 2 bytes were erased at offset 0x000001fe (vfat): 55 aa",
  ]
  assert image.read_bytes()? == expected.read_bytes()?
  let probed = run_real(ctx, "blkid", [f"{image}"])
  assert probed.status == 2
  assert probed.stdout == ""
  assert run_real(ctx, "wipefs", [f"{image}"]).stdout == ""
}

test test_storage_wipefs_lists_gpt_headers_before_the_protective_mbr { |ctx|
  let image = partitioned_image(ctx, "gpt")
  let listed = run_real(ctx, "wipefs", ["-O", "OFFSET,TYPE,LENGTH,USAGE", f"{image}"])
  assert listed.success, listed.stderr
  assert [line.words() for line in listed.stdout.lines()] == [
    ["OFFSET", "TYPE", "LENGTH", "USAGE"],
    ["0x200", "gpt", "8", "partition-table"],
    ["0x7ffe00", "gpt", "8", "partition-table"],
    ["0x1fe", "PMBR", "2", "partition-table"],
  ]
  let erased = run_real(ctx, "wipefs", ["--all", f"{image}"])
  assert erased.success, erased.stderr
  assert erased.stdout.lines() == [
    f"{image}: 8 bytes were erased at offset 0x00000200 (gpt): 45 46 49 20 50 41 52 54",
    f"{image}: 8 bytes were erased at offset 0x007ffe00 (gpt): 45 46 49 20 50 41 52 54",
    f"{image}: 2 bytes were erased at offset 0x000001fe (PMBR): 55 aa",
  ]
  assert run_real(ctx, "blkid", [f"{image}"]).status == 2
}

test test_storage_wipefs_names_an_unreadable_operand { |ctx|
  let dir = test.temp_dir(ctx, name: "wipefs-missing")?
  let missing = fp"{dir}/missing.img"
  let result = run_real(ctx, "wipefs", [f"{missing}"])
  assert result.status == 1
  assert result.stdout == ""
  assert diagnostics(result)[0].starts_with(f"error: {missing}: probing initialization failed: ")
}

test test_storage_wipefs_rejects_invalid_modes_before_erasing { |ctx|
  let image = test.temp_file(ctx, name: "wipefs-invalid.img", contents: b"XFSBowned-payload")?
  let source = fp"{ctx.core_dir}/wipefs.xsh".read_text()?
  for args in [["-a", "-J"], ["-o", "0xnothex"], ["-o", "99"]] {
    let output = test.run_script(ctx, source, args + [f"{image}"], {XSH_MODULE_PATH: ctx.core_dir}, b"", "wipefs")?
    assert ! output.success
    assert image.read_bytes()? == b"XFSBowned-payload"
  }
}

test test_storage_blockdev_rejects_regular_files_without_changing_them { |ctx|
  let image = test.temp_file(ctx, name: "blockdev-invalid.img", contents: b"owned-payload")?
  let source = fp"{ctx.core_dir}/blockdev.xsh".read_text()?
  for operation in ["--getsize64", "--setro", "--rereadpt", "--flushbufs"] {
    let output = test.run_script(ctx, source, [operation, f"{image}"], {XSH_MODULE_PATH: ctx.core_dir}, b"", "blockdev")?
    assert ! output.success
    assert image.read_bytes()? == b"owned-payload"
  }
}

test test_storage_fsfreeze_rejects_conflicting_modes_before_controller { |ctx|
  let log = test.temp_file(ctx, name: "fsfreeze-invalid-log", contents: b"")?
  let output = run_storage(ctx, "fsfreeze", ["-f", "-u", "/owned/mount"], log)
  assert ! output.success
  assert "exactly one" in output.stderr
  assert log.read_text()? == ""
}

test test_storage_sfdisk_roundtrips_owned_gpt_and_dos_images { |ctx|
  let source = fp"{ctx.core_dir}/sfdisk.xsh".read_text()?
  for label in ["gpt", "dos"] {
    let image = test.temp_file(ctx, name: f"sfdisk-{label}.img", contents: b"")?
    image.truncate(8388608)
    let input = f"label: {label}\nunit: sectors\nstart=2048,size=4096,type=L\n"
    let written = test.run_script(ctx, source, [f"{image}"], {XSH_MODULE_PATH: ctx.core_dir}, bytes.from_text(input), "sfdisk")?
    assert written.success, written.stderr
    let table = linux.partition_table(image)?
    assert table.label == label
    assert table.partitions.len() == 1
    assert table.partitions[0].start == 2048
    assert table.partitions[0].size == 4096
    if label == "gpt" {
      assert table.id != "00000000-0000-0000-0000-000000000000"
      assert table.partitions[0].uuid != "00000000-0000-0000-0000-000000000000"
    }
    let dump = test.run_script(ctx, source, ["--dump", f"{image}"], {XSH_MODULE_PATH: ctx.core_dir}, b"", "sfdisk")?
    assert dump.success, dump.stderr
    let restored = test.run_script(ctx, source, [f"{image}"], {XSH_MODULE_PATH: ctx.core_dir}, dump.stdout_bytes, "sfdisk")?
    assert restored.success, restored.stderr
    assert linux.partition_table(image)? == table
  }
}

test test_storage_mkswap_initializes_owned_file_and_rejects_fifo { |ctx|
  let image = test.temp_file(ctx, name: "owned-swap.img", contents: b"")?
  image.truncate(1048576)
  let source = fp"{ctx.core_dir}/mkswap.xsh".read_text()?
  let initialized = test.run_script(ctx, source, [f"{image}"], {XSH_MODULE_PATH: ctx.core_dir}, b"", "mkswap")?
  assert initialized.success, initialized.stderr
  assert linux.blkid(image)?.type == "swap"
  let dir = test.temp_dir(ctx, name: "swap-fifo")?
  let fifo = fp"{dir}/pipe"
  fs.mkfifo(fifo, 0o600)
  let refused = test.run_script(ctx, source, [f"{fifo}"], {XSH_MODULE_PATH: ctx.core_dir}, b"", "mkswap")?
  assert ! refused.success
  assert "regular file or block device" in refused.stderr
  assert fs.stat(fifo)?.kind == "fifo"
}

# Runs an applet against the real host primitives. Nothing here may reach a
# block device or mount point: every operand is an image file, a scratch
# directory or a path that cannot exist.
proc run_real(ctx: TestContext, name: Str, args: List[Str], stdin: Bytes = b"") -> StorageRun {
  let source = fp"{ctx.core_dir}/{name}.xsh".read_text()?
  test.run_script(ctx, source, args, {XSH_MODULE_PATH: ctx.core_dir}, stdin, name)?
}

# The diagnostics of one run with the leading `PROGRAM: ` removed; the test
# harness suffixes the program name with a run counter.
proc diagnostics(output: StorageRun) -> List[Str] {
  [line.split(": ", maxsplit: 1)[1] for line in output.stderr.lines()]
}

# Both layouts hold partitions at sectors 2048-6143 and 8192-10239 of an
# 8 MiB image, and their identifiers are fixed so listings are reproducible.
# The extents, ids and type codes are the values util-linux fdisk, sfdisk and
# partx report for the same written image.
proc partitioned_image(ctx: TestContext, label: Str) -> Path {
  let image = test.temp_file(ctx, name: f"{label}-listing.img", contents: b"")?
  image.truncate(8388608)
  let input = if label == "gpt" {
    "label: gpt\nlabel-id: 11111111-2222-3333-4444-555555555555\nunit: sectors\nstart=2048,size=4096,type=L,uuid=aaaaaaaa-0000-0000-0000-000000000001,name=\"root\"\nstart=8192,size=2048,type=U,uuid=aaaaaaaa-0000-0000-0000-000000000002\n"
  } else {
    "label: dos\nlabel-id: 0x1234abcd\nunit: sectors\nstart=2048,size=4096,type=L\nstart=8192,size=2048,type=ef\n"
  }
  let written = run_real(ctx, "sfdisk", [f"{image}"], bytes.from_text(input))
  assert written.success, written.stderr
  image
}

# The text util-linux fdisk prints above a table for the 8 MiB images of
# `partitioned_image`.
pure disk_heading(image: Path, label: Str, identifier: Str) -> List[Str] {
  [
    f"Disk {image}: 8 MiB, 8388608 bytes, 16384 sectors",
    "Units: sectors of 1 * 512 = 512 bytes",
    "Sector size (logical/physical): 512 bytes / 512 bytes",
    "I/O size (minimum/optimal): 512 bytes / 512 bytes",
    f"Disklabel type: {label}",
    f"Disk identifier: {identifier}",
  ]
}

# The device column is as wide as its longest node once that outgrows the
# heading, and every other field keeps util-linux's fixed alignment. Test image
# names end in a digit, so their partition nodes carry the `p` separator.
test test_storage_fdisk_list_reports_the_util_linux_layout_for_gpt_and_dos { |ctx|
  let gpt = partitioned_image(ctx, "gpt")
  let before = gpt.read_bytes()?
  let listed = run_real(ctx, "fdisk", ["-l", f"{gpt}"])
  assert listed.success, listed.stderr
  assert listed.stdout.lines() == disk_heading(gpt, "gpt", "11111111-2222-3333-4444-555555555555") + [
    "",
    tui.right_pad("Device", f"{gpt}p1".count_chars()) + " Start   End Sectors Size Type",
    f"{gpt}p1" + "  2048  6143    4096   2M Linux filesystem",
    f"{gpt}p2" + "  8192 10239    2048   1M EFI System",
  ]
  assert gpt.read_bytes()? == before

  let dos = partitioned_image(ctx, "dos")
  let boot = run_real(ctx, "fdisk", ["-l", f"{dos}"])
  assert boot.success, boot.stderr
  assert boot.stdout.lines() == disk_heading(dos, "dos", "0x1234abcd") + [
    "",
    tui.right_pad("Device", f"{dos}p1".count_chars()) + " Boot Start   End Sectors Size Id Type",
    f"{dos}p1" + "       2048  6143    4096   2M 83 Linux",
    f"{dos}p2" + "       8192 10239    2048   1M ef EFI (FAT-12/16/32)",
  ]
}

test test_storage_fdisk_list_selects_columns_and_names_the_ones_a_label_lacks { |ctx|
  let gpt = partitioned_image(ctx, "gpt")
  let identity = run_real(ctx, "fdisk", ["-l", "-o", "device,name,UUID,Type-UUID", f"{gpt}"])
  assert identity.success, identity.stderr
  let rows = identity.stdout.lines()
  assert rows.len() == 10
  assert rows[7].words() == ["Device", "Name", "UUID", "Type-UUID"]
  assert rows[8].words() == [f"{gpt}p1", "root", "AAAAAAAA-0000-0000-0000-000000000001", "0FC63DAF-8483-4772-8E79-3D69D8477DE4"]
  assert rows[9].words() == [f"{gpt}p2", "AAAAAAAA-0000-0000-0000-000000000002", "C12A7328-F81F-11D2-BA4B-00A0C93EC93B"]
  let extended = run_real(ctx, "fdisk", ["-l", "-o", "+Name", f"{gpt}"])
  assert extended.stdout.lines()[7].words() == ["Device", "Start", "End", "Sectors", "Size", "Type", "Name"]

  let dos = partitioned_image(ctx, "dos")
  let types = run_real(ctx, "fdisk", ["-l", "-o", "Device,Id,Type", f"{dos}"])
  assert types.success, types.stderr
  assert types.stdout.lines()[8].words() == [f"{dos}p1", "83", "Linux"]
  assert types.stdout.lines()[9].words() == [f"{dos}p2", "ef", "EFI", "(FAT-12/16/32)"]
  # The heading is printed before a column the label cannot supply is refused.
  let lacking = run_real(ctx, "fdisk", ["-l", "-o", "Device,Name", f"{dos}"])
  assert ! lacking.success
  assert lacking.stdout.lines() == disk_heading(dos, "dos", "0x1234abcd")
  assert diagnostics(lacking)[0] == "dos unknown column: Name"
}

test test_storage_fdisk_sizes_use_the_util_linux_unit_rounding { |ctx|
  let image = test.temp_file(ctx, name: "fdisk-sizes.img", contents: b"")?
  image.truncate(1048576)
  let written = run_real(ctx, "sfdisk", [f"{image}"], b"label: dos\nunit: sectors\nstart=1,size=1\nstart=10,size=5\nstart=20,size=6\nstart=100,size=1000\n")
  assert written.success, written.stderr
  let listed = run_real(ctx, "fdisk", ["-l", "-o", "Sectors,Size", f"{image}"])
  assert listed.success, listed.stderr
  assert listed.stdout.lines()[0] == f"Disk {image}: 1 MiB, 1048576 bytes, 2048 sectors"
  let rows = listed.stdout.lines()
  assert [rows[at].words().join(" ") for at in range(8, rows.len())] == ["1 512B", "5 2.5K", "6 3K", "1000 500K"]
}

test test_storage_partition_listings_agree_on_extents { |ctx|
  for label in ["gpt", "dos"] {
    let image = partitioned_image(ctx, label)
    let fdisk = run_real(ctx, "fdisk", ["-l", "-o", "START,SECTORS", f"{image}"])
    let partx = run_real(ctx, "partx", ["-s", "--noheadings", "-o", "START,SECTORS", f"{image}"])
    assert fdisk.success, fdisk.stderr
    assert partx.success, partx.stderr
    assert partx.stdout == "2048 4096\n8192 2048\n"
    let fdisk_rows = fdisk.stdout.lines()
    assert fdisk_rows.len() == 10
    assert [fdisk_rows[8].words().join(" "), fdisk_rows[9].words().join(" ")] == partx.stdout.lines()
    let json_listing = run_real(ctx, "sfdisk", ["-J", f"{image}"])
    assert json_listing.success, json_listing.stderr
    let table = json.decode(json_listing.stdout)?.require(ListedDisk)?
    assert table.partitiontable.label == label
    assert [item.start for item in table.partitiontable.partitions] == [2048, 8192]
    assert [item.size for item in table.partitiontable.partitions] == [4096, 2048]
  }
}

test test_storage_fdisk_lists_unpartitioned_image_without_rows { |ctx|
  let image = test.temp_file(ctx, name: "fdisk-blank.img", contents: b"")?
  image.truncate(1048576)
  let listed = run_real(ctx, "fdisk", ["-l", f"{image}"])
  assert listed.success, listed.stderr
  assert listed.stdout == f"Disk {image}: 1 MiB, 1048576 bytes, 2048 sectors\nUnits: sectors of 1 * 512 = 512 bytes\nSector size (logical/physical): 512 bytes / 512 bytes\nI/O size (minimum/optimal): 512 bytes / 512 bytes\n"
}

test test_storage_fdisk_separates_several_devices_with_two_blank_lines { |ctx|
  let gpt = partitioned_image(ctx, "gpt")
  let dos = partitioned_image(ctx, "dos")
  let listed = run_real(ctx, "fdisk", ["-l", f"{gpt}", f"{dos}"])
  assert listed.success, listed.stderr
  let rows = listed.stdout.lines()
  assert rows[10] == "" and rows[11] == ""
  assert rows[12] == f"Disk {dos}: 8 MiB, 8388608 bytes, 16384 sectors"
}

# sfdisk --dump is the text sfdisk reads back, so its field layout is exact:
# only the headers a label has, start and size padded to twelve columns, and
# only the per-partition fields that exist.
test test_storage_sfdisk_dump_matches_the_util_linux_layout { |ctx|
  let gpt = partitioned_image(ctx, "gpt")
  let dump = run_real(ctx, "sfdisk", ["--dump", f"{gpt}"])
  assert dump.success, dump.stderr
  assert dump.stdout == f"label: gpt\nlabel-id: 11111111-2222-3333-4444-555555555555\ndevice: {gpt}\nunit: sectors\nfirst-lba: 34\nlast-lba: 16350\nsector-size: 512\n\n{gpt}p1 : start=        2048, size=        4096, type=0FC63DAF-8483-4772-8E79-3D69D8477DE4, uuid=AAAAAAAA-0000-0000-0000-000000000001, name=\"root\"\n{gpt}p2 : start=        8192, size=        2048, type=C12A7328-F81F-11D2-BA4B-00A0C93EC93B, uuid=AAAAAAAA-0000-0000-0000-000000000002\n"

  let dos = partitioned_image(ctx, "dos")
  let _ = bytes.write_at(dos, 446, b"\x80")?
  let flagged = run_real(ctx, "sfdisk", ["-d", f"{dos}"])
  assert flagged.success, flagged.stderr
  assert flagged.stdout == f"label: dos\nlabel-id: 0x1234abcd\ndevice: {dos}\nunit: sectors\nsector-size: 512\n\n{dos}p1 : start=        2048, size=        4096, type=83, bootable\n{dos}p2 : start=        8192, size=        2048, type=ef\n"
  let table = run_real(ctx, "fdisk", ["-l", "-o", "Device,Boot", f"{dos}"])
  assert table.stdout.lines()[8].words() == [f"{dos}p1", "*"]
  assert table.stdout.lines()[9].words() == [f"{dos}p2"]
}

test test_storage_sfdisk_dump_records_the_small_disk_grain { |ctx|
  let image = test.temp_file(ctx, name: "sfdisk-grain.img", contents: b"")?
  image.truncate(2097152)
  let written = run_real(ctx, "sfdisk", [f"{image}"], b"label: dos\nlabel-id: 0x00c0ffee\nunit: sectors\nstart=34,size=100,type=c\n")
  assert written.success, written.stderr
  let dump = run_real(ctx, "sfdisk", ["-d", f"{image}"])
  assert dump.stdout == f"label: dos\nlabel-id: 0x00c0ffee\ndevice: {image}\nunit: sectors\ngrain: 512\nsector-size: 512\n\n{image}p1 : start=          34, size=         100, type=c\n"
  let json_dump = run_real(ctx, "sfdisk", ["--json", f"{image}"])
  assert json_dump.stdout == f"{{\n   \"partitiontable\": {{\n      \"label\": \"dos\",\n      \"id\": \"0x00c0ffee\",\n      \"device\": {json.encode(f"{image}")?},\n      \"unit\": \"sectors\",\n      \"grain\": \"512\",\n      \"sectorsize\": 512,\n      \"partitions\": [\n         {{\n            \"node\": {json.encode(f"{image}p1")?},\n            \"start\": 34,\n            \"size\": 100,\n            \"type\": \"c\"\n         }}\n      ]\n   }}\n}}\n"
  # Restoring a dump of a small disk must accept its grain header.
  let restored = run_real(ctx, "sfdisk", [f"{image}"], dump.stdout_bytes)
  assert restored.success, restored.stderr
  assert run_real(ctx, "sfdisk", ["-d", f"{image}"]).stdout == dump.stdout
}

test test_storage_sfdisk_json_is_the_util_linux_machine_interface { |ctx|
  let gpt = partitioned_image(ctx, "gpt")
  let listed = run_real(ctx, "sfdisk", ["--json", f"{gpt}"])
  assert listed.success, listed.stderr
  assert listed.stdout == f"{{\n   \"partitiontable\": {{\n      \"label\": \"gpt\",\n      \"id\": \"11111111-2222-3333-4444-555555555555\",\n      \"device\": {json.encode(f"{gpt}")?},\n      \"unit\": \"sectors\",\n      \"firstlba\": 34,\n      \"lastlba\": 16350,\n      \"sectorsize\": 512,\n      \"partitions\": [\n         {{\n            \"node\": {json.encode(f"{gpt}p1")?},\n            \"start\": 2048,\n            \"size\": 4096,\n            \"type\": \"0FC63DAF-8483-4772-8E79-3D69D8477DE4\",\n            \"uuid\": \"AAAAAAAA-0000-0000-0000-000000000001\",\n            \"name\": \"root\"\n         }},{{\n            \"node\": {json.encode(f"{gpt}p2")?},\n            \"start\": 8192,\n            \"size\": 2048,\n            \"type\": \"C12A7328-F81F-11D2-BA4B-00A0C93EC93B\",\n            \"uuid\": \"AAAAAAAA-0000-0000-0000-000000000002\"\n         }}\n      ]\n   }}\n}}\n"
}

test test_storage_sfdisk_dump_refuses_an_image_without_a_partition_table { |ctx|
  let image = test.temp_file(ctx, name: "sfdisk-blank.img", contents: b"")?
  image.truncate(1048576)
  for flag in ["-d", "-J"] {
    let refused = run_real(ctx, "sfdisk", [flag, f"{image}"])
    assert refused.status == 1
    assert refused.stdout == ""
    assert diagnostics(refused) == [f"{image}: does not contain a recognized partition table"]
  }
}

test test_storage_fdisk_refuses_editing_and_unsupported_listing_modes_without_writing { |ctx|
  let image = partitioned_image(ctx, "gpt")
  let before = image.read_bytes()?
  let session = run_real(ctx, "fdisk", [f"{image}"], b"d\n1\nw\n")
  assert ! session.success
  assert "interactive fdisk editing" in session.stderr
  assert image.read_bytes()? == before
  for flag in ["-lu", "-x", "-lx"] {
    let refused = run_real(ctx, "fdisk", [flag, f"{image}"])
    assert ! refused.success
    assert "is not supported" in refused.stderr
    assert refused.stdout == ""
  }
  let unknown = run_real(ctx, "fdisk", ["-l", "-o", "BOGUS", f"{image}"])
  assert ! unknown.success
  assert "unknown column: BOGUS" in unknown.stderr
  assert image.read_bytes()? == before
}

test test_storage_fdisk_names_the_operand_it_cannot_open { |ctx|
  let dir = test.temp_dir(ctx, name: "fdisk-operands")?
  let missing = fp"{dir}/missing.img"
  let result = run_real(ctx, "fdisk", ["-l", f"{missing}"])
  assert ! result.success
  assert diagnostics(result)[0].starts_with(f"cannot open {missing}: ")
  assert result.stdout == ""
  let directory = run_real(ctx, "fdisk", ["-l", f"{dir}"])
  assert ! directory.success
  assert f"{dir}" in directory.stderr
  let empty = run_real(ctx, "fdisk", ["-l"])
  assert ! empty.success
  assert "missing operand" in empty.stderr
}

test test_storage_partx_lists_and_shows_the_same_reference_extents { |ctx|
  for label in ["gpt", "dos"] {
    let image = partitioned_image(ctx, label)
    for mode in ["-l", "-s"] {
      let shown = run_real(ctx, "partx", [mode, "-o", "NR,START,END,SECTORS", f"{image}"])
      assert shown.success, shown.stderr
      assert shown.stdout == "NR START END SECTORS\n1 2048 6143 4096\n2 8192 10239 2048\n"
    }
    let bare = run_real(ctx, "partx", ["--noheadings", "-o", "NR,START", f"{image}"])
    assert bare.success, bare.stderr
    assert bare.stdout == "1 2048\n2 8192\n"
  }
}

test test_storage_partx_carries_gpt_partition_identity { |ctx|
  let image = partitioned_image(ctx, "gpt")
  let shown = run_real(ctx, "partx", ["-s", "--noheadings", "-o", "NR,NAME,UUID", f"{image}"])
  assert shown.success, shown.stderr
  let rows = shown.stdout.lines()
  assert rows[0] == "1 root aaaaaaaa-0000-0000-0000-000000000001"
  assert rows[1].ends_with("aaaaaaaa-0000-0000-0000-000000000002")
}

test test_storage_partx_requires_a_partition_table_and_names_the_operand { |ctx|
  let blank = test.temp_file(ctx, name: "partx-blank.img", contents: b"")?
  blank.truncate(1048576)
  let unpartitioned = run_real(ctx, "partx", ["-l", f"{blank}"])
  assert ! unpartitioned.success
  assert diagnostics(unpartitioned) == [f"{blank}: failed to read partition table"]
  assert unpartitioned.stdout == ""

  let dir = test.temp_dir(ctx, name: "partx-operands")?
  let missing = fp"{dir}/missing.img"
  let absent = run_real(ctx, "partx", ["-s", f"{missing}"])
  assert ! absent.success
  assert diagnostics(absent)[0].starts_with(f"{missing}: ")
  assert "No such file or directory" in absent.stderr

  let tiny = test.temp_file(ctx, name: "partx-tiny.img", contents: b"")?
  let too_small = run_real(ctx, "partx", ["-l", f"{tiny}"])
  assert ! too_small.success
  assert diagnostics(too_small)[0].starts_with(f"{tiny}: ")
}

test test_storage_partx_refuses_kernel_update_modes_before_reading { |ctx|
  let image = partitioned_image(ctx, "gpt")
  for flag in ["-g", "-a", "-d", "-u", "-n"] {
    let refused = run_real(ctx, "partx", [flag, f"{image}"])
    assert ! refused.success, flag
    assert "not supported" in refused.stderr
    assert refused.stdout == ""
  }
}

test test_storage_partprobe_dry_run_summarizes_partition_numbers { |ctx|
  for label in ["gpt", "dos"] {
    let image = partitioned_image(ctx, label)
    let before = image.read_bytes()?
    let summary = run_real(ctx, "partprobe", ["-d", "-s", f"{image}"])
    assert summary.success, summary.stderr
    assert summary.stdout == f"{image}: {label} partitions 1 2\n"
    let silent = run_real(ctx, "partprobe", ["-d", f"{image}"])
    assert silent.success, silent.stderr
    assert silent.stdout == ""
    assert image.read_bytes()? == before
  }
}

test test_storage_partprobe_summary_of_unpartitioned_image_has_no_partition_numbers { |ctx|
  let blank = test.temp_file(ctx, name: "partprobe-blank.img", contents: b"")?
  blank.truncate(1048576)
  let summary = run_real(ctx, "partprobe", ["-d", "-s", f"{blank}"])
  assert summary.success, summary.stderr
  assert summary.stdout == f"{blank}: none partitions\n"
}

test test_storage_partprobe_refuses_non_block_files_without_summary { |ctx|
  let image = partitioned_image(ctx, "gpt")
  let before = image.read_bytes()?
  let refused = run_real(ctx, "partprobe", ["-s", f"{image}"])
  assert ! refused.success
  # The kernel's wording for a reread request on a regular file depends on
  # the libc that renders ENOTTY.
  assert diagnostics(refused)[0].starts_with(f"{image}: ")
  assert "Not a tty" in refused.stderr or "Inappropriate ioctl for device" in refused.stderr
  assert refused.stdout == ""
  assert image.read_bytes()? == before

  let dir = test.temp_dir(ctx, name: "partprobe-operands")?
  let missing = fp"{dir}/missing.img"
  let absent = run_real(ctx, "partprobe", ["-d", f"{missing}"])
  assert ! absent.success
  assert diagnostics(absent)[0].starts_with(f"{missing}: ")
  assert "No such file or directory" in absent.stderr
  let nothing = run_real(ctx, "partprobe", ["-d"])
  assert ! nothing.success
  assert "missing operand" in nothing.stderr
}

test test_storage_fstrim_names_missing_mountpoint_with_kernel_wording { |ctx|
  let dir = test.temp_dir(ctx, name: "fstrim-operands")?
  let missing = fp"{dir}/missing"
  let result = run_real(ctx, "fstrim", ["-v", f"{missing}"])
  assert ! result.success
  assert diagnostics(result) == [f"stat of {missing} failed: No such file or directory"]
  assert result.stdout == ""
}

test test_storage_fstrim_reports_filesystems_without_discard_support { |ctx|
  guard p"/proc/self".exists()? else {
    test.skip("needs procfs, which has no discard ioctl")
    return
  }
  # procfs rejects the discard request itself, so no real filesystem is
  # trimmed and the result does not depend on privilege.
  let result = run_real(ctx, "fstrim", ["-v", "/proc/self"])
  assert ! result.success
  assert diagnostics(result) == ["/proc/self: the discard operation is not supported"]
  assert result.stdout == ""
}

test test_storage_fstrim_rejects_bad_operands_before_the_kernel { |ctx|
  let dir = test.temp_dir(ctx, name: "fstrim-invalid")?
  let none = run_real(ctx, "fstrim", [])
  assert ! none.success
  assert diagnostics(none) == ["no mountpoint specified"]
  let extra = run_real(ctx, "fstrim", [f"{dir}", f"{dir}"])
  assert ! extra.success
  assert "unexpected number of arguments" in extra.stderr
  for case in [["-l", "abc"], ["-o", "-1"], ["-m", "12x"]] {
    let invalid = run_real(ctx, "fstrim", case + [f"{dir}"])
    assert ! invalid.success
    assert diagnostics(invalid) == [f"failed to parse {if case[0] == "-l" { "length" } else if case[0] == "-o" { "offset" } else { "minimum extent length" }}: '{case[1]}': Invalid argument"]
    assert invalid.stdout == ""
  }
  let unknown = run_real(ctx, "fstrim", ["--all", f"{dir}"])
  assert ! unknown.success
  assert "not supported" in unknown.stderr
}

test test_storage_umount_names_every_operand_that_is_not_a_mount_point { |ctx|
  let dir = test.temp_dir(ctx, name: "umount-operands")?
  let file = test.temp_file(ctx, name: "umount-file", contents: b"retained")?
  let missing = fp"{dir}/missing"
  let result = run_real(ctx, "umount", [f"{dir}", f"{file}", f"{missing}"])
  assert ! result.success
  assert result.stdout == ""
  let lines = diagnostics(result)
  assert lines.len() == 3
  # An unprivileged caller is refused before the kernel inspects the path;
  # a privileged one learns that the path is not a mount point.
  for index in [0, 1] {
    let name = if index == 0 { f"{dir}" } else { f"{file}" }
    assert lines[index].starts_with(f"{name}: ")
    assert lines[index].ends_with("Operation not permitted") or lines[index].ends_with("Invalid argument")
  }
  assert lines[2] == f"{missing}: No such file or directory"
  assert file.read_bytes()? == b"retained"
  assert dir.exists()?
}

test test_storage_umount_lazy_and_force_keep_the_diagnostic_shape { |ctx|
  let dir = test.temp_dir(ctx, name: "umount-modes")?
  for flag in ["-l", "-f"] {
    let result = run_real(ctx, "umount", [flag, f"{dir}"])
    assert ! result.success
    assert diagnostics(result)[0].starts_with(f"{dir}: ")
    assert result.stdout == ""
  }
}

test test_storage_umount_refuses_unsupported_selections_before_acting { |ctx|
  let log = test.temp_file(ctx, name: "umount-refused-log", contents: b"")?
  let dir = test.temp_dir(ctx, name: "umount-refused")?
  for args in [["-a", "-l"], ["-a", "-f"], ["-a", f"{dir}"], ["-t", "ext4", f"{dir}"], [], ["--bogus", f"{dir}"]] {
    let result = run_storage(ctx, "umount", args, log)
    assert ! result.success
    assert result.stdout == ""
  }
  assert log.read_text()? == ""
}

test test_storage_umount_all_filters_by_type_through_the_fake { |ctx|
  let log = test.temp_file(ctx, name: "umount-all-log", contents: b"")?
  let result = run_storage(ctx, "umount", ["-a", "-t", "ext4,xfs"], log)
  assert result.success, result.stderr
  let recorded = log.read_text()?
  assert "\"op\":\"umount_all\"" in recorded
  assert "ext4" in recorded and "xfs" in recorded
}
