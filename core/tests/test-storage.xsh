use core.lib.storage

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

test test_storage_partition_json_is_conventional_machine_interface { |ctx|
  let log = test.temp_file(ctx, name: "partition-json-log", contents: b"")?
  let output = run_storage(ctx, "sfdisk", ["--json", "/owned/disk"], log)
  assert output.success, output.stderr
  assert "\"partitiontable\"" in output.stdout
  assert "\"sectorsize\"" in output.stdout
  assert "\"start\"" in output.stdout and "\"size\"" in output.stdout
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
    root.symlink(fp"../../devices/disk/{if name == "disk" { "" } else { "disk1" }}", fp"sys/class/block/{name}")
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
  assert "\"type\":\"xfs\"" in listing.stdout
  assert "\"offset\":\"0x0\"" in listing.stdout
  let dry = test.run_script(ctx, source, ["--no-act", "--offset", "0x0", f"{image}"], {XSH_MODULE_PATH: ctx.core_dir}, b"", "wipefs")?
  assert dry.success, dry.stderr
  assert image.read_bytes()? == b"XFSBpayload-preserved"
  let erased = test.run_script(ctx, source, ["--offset", "0x0", f"{image}"], {XSH_MODULE_PATH: ctx.core_dir}, b"", "wipefs")?
  assert erased.success, erased.stderr
  assert image.read_bytes()? == b"\0\0\0\0payload-preserved"
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
