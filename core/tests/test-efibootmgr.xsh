type Run = {success: Bool, status: Int, stdout: Str, stderr: Str, stdout_bytes: Bytes, stderr_bytes: Bytes}

const GUID_SUFFIX = "-8be4df61-93ca-11d2-aa0d-00e098032b8c"

# Boot manager variables are written with attributes NV|BS|RT (7).
proc put(dir: Path, name: Str, data: Bytes) [fs, error] {
  fp"{dir}/{name}{GUID_SUFFIX}".write(bytes.concat([bytes.from_ints([7, 0, 0, 0])?, data]))?
}

proc get(dir: Path, name: Str) [fs, error] -> Bytes {
  fp"{dir}/{name}{GUID_SUFFIX}".read_bytes()?
}

proc exists(dir: Path, name: Str) [fs, error] -> Bool {
  fp"{dir}/{name}{GUID_SUFFIX}".exists()?
}

proc u16(value: Int) -> Bytes { bytes.pack_le(value, 2)? }

proc u16_list(values: List[Int]) -> Bytes {
  var chunks: List[Bytes] = []
  for value in values { chunks += [u16(value)] }
  bytes.concat(chunks)
}

proc utf16z(text: Str) -> Bytes {
  var ints: List[Int] = []
  for byte in bytes.from_text(text) { ints += [byte, 0] }
  bytes.concat([bytes.from_ints(ints)?, b"\x00\x00"])
}

proc node(kind: Int, subtype: Int, data: Bytes) -> Bytes {
  bytes.concat([bytes.from_ints([kind, subtype])?, u16(data.len() + 4), data])
}

proc hard_drive(part: Int, start: Int, size: Int, signature: List[Int], table: Int) -> Bytes {
  node(4, 1, bytes.concat([bytes.pack_le(part, 4)?, bytes.pack_le(start, 8)?, bytes.pack_le(size, 8)?, bytes.from_ints(signature)?, bytes.from_ints([table, table])?]))
}

const GPT_SIGNATURE = [120, 86, 52, 18, 52, 18, 120, 86, 18, 52, 86, 120, 18, 52, 86, 120]
const END = b"\x7f\xff\x04\x00"

proc load_option(attributes: Int, description: Str, nodes: Bytes, data: Bytes) -> Bytes {
  let list = bytes.concat([nodes, END])
  bytes.concat([bytes.pack_le(attributes, 4)?, u16(list.len()), utf16z(description), list, data])
}

proc linux_entry(label: Str, loader: Str, data: Bytes) -> Bytes {
  load_option(1, label, bytes.concat([hard_drive(1, 2048, 1048576, GPT_SIGNATURE, 2), node(4, 4, utf16z(loader))]), data)
}

# Boot0000 Linux (active), Boot0001 Windows (active, binary data), Boot0002
# Shell (inactive, no disk node), with BootOrder 1,0,2.
proc fixture(ctx: TestContext, name: Str) [fs, error] -> Path {
  let dir = test.temp_dir(ctx, name: name)?
  put(dir, "BootOrder", u16_list([1, 0, 2]))
  put(dir, "BootCurrent", u16(1))
  put(dir, "Timeout", u16(5))
  put(dir, "Boot0000", linux_entry("Linux", "\\EFI\\linux\\grub.efi", b""))
  put(dir, "Boot0001", linux_entry("Windows Boot Manager", "\\EFI\\Microsoft\\Boot\\bootmgfw.efi", b"WINDOWS\x00\x01\x00"))
  put(dir, "Boot0002", load_option(0, "Shell", node(4, 4, utf16z("\\shell.efi")), b""))
  dir
}

proc invoke_with(ctx: TestContext, dir: Path, argv: List[Str], allow_any_dir: Bool, stdin: Bytes) [fs, process, error] -> Run {
  let source = fp"{ctx.core_dir}/efibootmgr.xsh".read_text()?
  if allow_any_dir {
    return test.run_script(ctx, source, argv, {XSH_MODULE_PATH: ctx.core_dir.display(), EFIVARFS_PATH: dir.display(), EFIBOOTMGR_ALLOW_ANY_DIR: "1"}, stdin, "efibootmgr")?
  }
  test.run_script(ctx, source, argv, {XSH_MODULE_PATH: ctx.core_dir.display(), EFIVARFS_PATH: dir.display()}, stdin, "efibootmgr")?
}

proc invoke(ctx: TestContext, dir: Path, argv: List[Str]) [fs, process, error] -> Run {
  invoke_with(ctx, dir, argv, false, b"")
}

const HD_LINUX = "HD(1,GPT,12345678-1234-5678-1234-567812345678,0x800,0x100000)"

test test_efibootmgr_lists_state_and_load_options { |ctx|
  let dir = fixture(ctx, "efibootmgr-list")
  let before = names_in(dir).len()
  assert before == 6
  let listed = invoke(ctx, dir, [])
  assert listed.status == 0, listed.stderr
  assert listed.stdout == f"BootCurrent: 0001\nTimeout: 5 seconds\nBootOrder: 0001,0000,0002\nBoot0000* Linux\t{HD_LINUX}/\\EFI\\linux\\grub.efi\nBoot0001* Windows Boot Manager\t{HD_LINUX}/\\EFI\\Microsoft\\Boot\\bootmgfw.efi57494e444f5753000100\nBoot0002  Shell\t\\shell.efi\n"
  assert names_in(dir).len() == before
}

test test_efibootmgr_verbose_shows_device_path_bytes_and_optional_data { |ctx|
  let dir = fixture(ctx, "efibootmgr-verbose")
  let listed = invoke(ctx, dir, ["-v"])
  assert listed.status == 0, listed.stderr
  assert "      dp: 04 01 2a 00 01 00 00 00 00 08 00 00 00 00 00 00 00 00 10 00 00 00 00 00 78 56 34 12 34 12 78 56 12 34 56 78 12 34 56 78 02 02 / 04 04 2c 00" in listed.stdout
  assert " / 7f ff 04 00\n" in listed.stdout
  assert "    data: 57 49 4e 44 4f 57 53 00 01 00\n" in listed.stdout
}

test test_efibootmgr_prints_ascii_optional_data_as_text_and_binary_as_hex { |ctx|
  let dir = fixture(ctx, "efibootmgr-data")
  put(dir, "Boot0000", linux_entry("Linux", "\\EFI\\linux\\grub.efi", b"root=/dev/sda1 quiet"))
  put(dir, "Boot0002", linux_entry("Shell", "\\shell.efi", b"a\x00"))
  let listed = invoke(ctx, dir, [])
  assert listed.status == 0, listed.stderr
  assert f"Boot0000* Linux\t{HD_LINUX}/\\EFI\\linux\\grub.efiroot=/dev/sda1 quiet\n" in listed.stdout
  assert f"Boot0002* Shell\t{HD_LINUX}/\\shell.efi6100\n" in listed.stdout
}

test test_efibootmgr_formats_mbr_pci_sata_and_unknown_nodes { |ctx|
  let dir = test.temp_dir(ctx, name: "efibootmgr-nodes")?
  let mbr = hard_drive(3, 2048, 65536, [239, 190, 173, 222, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], 1)
  let root = node(2, 1, bytes.concat([bytes.pack_le(168313296, 4)?, bytes.pack_le(0, 4)?]))
  let pci = node(1, 1, b"\x02\x1f")
  let sata = node(3, 18, bytes.concat([u16(0), u16(65535), u16(0)]))
  put(dir, "BootOrder", u16_list([0, 1]))
  put(dir, "Boot0000", load_option(1, "mbr", bytes.concat([mbr, node(4, 4, utf16z("\\b.efi"))]), b""))
  put(dir, "Boot0001", load_option(1, "hw", bytes.concat([root, pci, sata, node(5, 9, b"ab")]), b""))
  let listed = invoke(ctx, dir, [])
  assert listed.status == 0, listed.stderr
  assert "Boot0000* mbr\tHD(3,MBR,0xdeadbeef,0x800,0x10000)/\\b.efi\n" in listed.stdout
  assert "Boot0001* hw\tPcieRoot(0x0)/Pci(0x1f,0x2)/Sata(0,65535,0)/BbsPath(9,6162)\n" in listed.stdout
}

test test_efibootmgr_reports_missing_boot_order_and_unparseable_entries { |ctx|
  let dir = test.temp_dir(ctx, name: "efibootmgr-empty")?
  let empty = invoke(ctx, dir, [])
  assert empty.status == 0, empty.stderr
  assert empty.stdout == "No BootOrder is set; firmware will attempt recovery\n"
  put(dir, "Boot0007", b"\x01\x00")
  let broken = invoke(ctx, dir, [])
  assert broken.status == 0, broken.stderr
  assert "Boot0007  Could not parse load option: load option is shorter than its header\n" in broken.stdout
}

test test_efibootmgr_boot_order_writes_whole_variable_and_validates_entries { |ctx|
  let dir = fixture(ctx, "efibootmgr-order")
  let written = invoke(ctx, dir, ["-o", "0002,0000"])
  assert written.status == 0, written.stderr
  assert get(dir, "BootOrder") == bytes.concat([b"\x07\x00\x00\x00", u16_list([2, 0])])
  assert "BootOrder: 0002,0000\n" in written.stdout
  let bad = invoke(ctx, dir, ["-o", "0000,0009"])
  assert bad.status == 8
  assert bad.stderr == "Invalid BootOrder order entry value0000,0009\n                                          ^\nefibootmgr: entry 0009 does not exist\n"
  assert get(dir, "BootOrder") == bytes.concat([b"\x07\x00\x00\x00", u16_list([2, 0])])
  let malformed = invoke(ctx, dir, ["-o", "0000,,0001"])
  assert malformed.status == 8
  assert malformed.stderr == "Malformed BootOrder order0000,,0001\n                                ^\n"
  let deleted = invoke(ctx, dir, ["-O"])
  assert deleted.status == 0, deleted.stderr
  assert ! exists(dir, "BootOrder")
  assert "No BootOrder is set; firmware will attempt recovery\n" in deleted.stdout
}

test test_efibootmgr_next_timeout_set_and_delete { |ctx|
  let dir = fixture(ctx, "efibootmgr-next")
  let next = invoke(ctx, dir, ["-n", "0002", "-t", "30"])
  assert next.status == 0, next.stderr
  assert get(dir, "BootNext") == bytes.concat([b"\x07\x00\x00\x00", u16(2)])
  assert get(dir, "Timeout") == bytes.concat([b"\x07\x00\x00\x00", u16(30)])
  assert next.stdout.starts_with("BootNext: 0002\nBootCurrent: 0001\nTimeout: 30 seconds\n")
  let missing = invoke(ctx, dir, ["-n", "0009"])
  assert missing.status == 12
  assert missing.stderr == "Boot entry 9 does not exist\n"
  let cleared = invoke(ctx, dir, ["-N", "-T"])
  assert cleared.status == 0, cleared.stderr
  assert ! exists(dir, "BootNext")
  assert ! exists(dir, "Timeout")
  let again = invoke(ctx, dir, ["-N"])
  assert again.status == 10
  assert again.stderr == "Could not delete BootNext: No such file or directory\n"
}

test test_efibootmgr_timeout_wraps_to_sixteen_bits_and_rejects_text { |ctx|
  let dir = fixture(ctx, "efibootmgr-timeout")
  assert invoke(ctx, dir, ["-t", "-1"]).status == 0
  assert get(dir, "Timeout") == bytes.concat([b"\x07\x00\x00\x00", u16(65535)])
  assert invoke(ctx, dir, ["-t", "65536"]).status == 0
  assert get(dir, "Timeout") == bytes.concat([b"\x07\x00\x00\x00", u16(0)])
  let bad = invoke(ctx, dir, ["-t", "soon"])
  assert bad.status == 38
  assert bad.stderr == "invalid numeric value soon\n\n"
}

test test_efibootmgr_delete_entry_removes_variable_and_order_entries { |ctx|
  let dir = fixture(ctx, "efibootmgr-delete")
  let deleted = invoke(ctx, dir, ["-B", "-b", "0001"])
  assert deleted.status == 0, deleted.stderr
  assert ! exists(dir, "Boot0001")
  assert get(dir, "BootOrder") == bytes.concat([b"\x07\x00\x00\x00", u16_list([0, 2])])
  assert "BootOrder: 0000,0002\n" in deleted.stdout
  let absent = invoke(ctx, dir, ["-B", "-b", "0001"])
  assert absent.status == 15
  assert absent.stderr == "Could not delete variable: No such file or directory\n"
  assert invoke(ctx, dir, ["-B"]).status == 3
}

test test_efibootmgr_delete_by_label_removes_every_match { |ctx|
  let dir = fixture(ctx, "efibootmgr-delete-label")
  put(dir, "Boot0003", linux_entry("Linux", "\\EFI\\linux\\grub2.efi", b""))
  put(dir, "BootOrder", u16_list([3, 1, 0, 2]))
  let deleted = invoke(ctx, dir, ["-B", "-L", "Linux"])
  assert deleted.status == 0, deleted.stderr
  assert ! exists(dir, "Boot0000")
  assert ! exists(dir, "Boot0003")
  assert exists(dir, "Boot0001")
  assert get(dir, "BootOrder") == bytes.concat([b"\x07\x00\x00\x00", u16_list([1, 2])])
  let none = invoke(ctx, dir, ["-B", "-L", "Linux"])
  assert none.status == 15
  assert none.stderr == "Could not delete variable\n"
}

test test_efibootmgr_active_state_and_driver_reconnect_bits { |ctx|
  let dir = fixture(ctx, "efibootmgr-active")
  let enabled = invoke(ctx, dir, ["-b", "0002", "-a"])
  assert enabled.status == 0, enabled.stderr
  assert "Boot0002* Shell\t" in enabled.stdout
  let disabled = invoke(ctx, dir, ["-b", "0002", "-A"])
  assert disabled.status == 0, disabled.stderr
  assert "Boot0002  Shell\t" in disabled.stdout
  assert get(dir, "Boot0002").slice(0, length: 8) == b"\x07\x00\x00\x00\x00\x00\x00\x00"
  let absent = invoke(ctx, dir, ["-b", "0009", "-a"])
  assert absent.status == 16
  assert absent.stderr == "efibootmgr: Boot entry 9 not found\nCould not set active state for Boot0009: No such file or directory\n"
  assert invoke(ctx, dir, ["-a"]).stderr == "You must specify a entry to activate (see the -b option)\n"
  put(dir, "Driver0000", load_option(1, "Drv", node(4, 4, utf16z("\\drv.efi")), b""))
  put(dir, "DriverOrder", u16_list([0]))
  let reconnect = invoke(ctx, dir, ["-r", "-b", "0000", "-f"])
  assert reconnect.status == 0, reconnect.stderr
  assert get(dir, "Driver0000").slice(0, length: 8) == b"\x07\x00\x00\x00\x03\x00\x00\x00"
  assert reconnect.stdout.starts_with("DriverOrder: 0000\nDriver0000* Drv\t\\drv.efi\n")
  assert invoke(ctx, dir, ["-r", "-b", "0000", "-F"]).status == 0
  assert get(dir, "Driver0000").slice(0, length: 8) == b"\x07\x00\x00\x00\x01\x00\x00\x00"
  assert invoke(ctx, dir, ["-f", "-b", "0000"]).stderr == "--reconnect is supported only for driver entries.\n"
}

test test_efibootmgr_remove_duplicates_keeps_first_occurrence { |ctx|
  let dir = fixture(ctx, "efibootmgr-dups")
  put(dir, "BootOrder", u16_list([0, 1, 0, 2, 1]))
  let result = invoke(ctx, dir, ["-D"])
  assert result.status == 0, result.stderr
  assert get(dir, "BootOrder") == bytes.concat([b"\x07\x00\x00\x00", u16_list([0, 1, 2])])
}

test test_efibootmgr_driver_and_sysprep_use_their_own_variables { |ctx|
  let dir = fixture(ctx, "efibootmgr-families")
  put(dir, "SysPrepOrder", u16_list([1, 0]))
  put(dir, "SysPrep0000", load_option(1, "Prep0", node(4, 4, utf16z("\\p0.efi")), b""))
  put(dir, "SysPrep0001", load_option(1, "Prep1", node(4, 4, utf16z("\\p1.efi")), b""))
  let listed = invoke(ctx, dir, ["-y"])
  assert listed.status == 0, listed.stderr
  assert listed.stdout == "SysPrepOrder: 0001,0000\nSysPrep0000* Prep0\t\\p0.efi\nSysPrep0001* Prep1\t\\p1.efi\n"
  let reordered = invoke(ctx, dir, ["-y", "-o", "0000"])
  assert reordered.status == 0, reordered.stderr
  assert get(dir, "SysPrepOrder") == bytes.concat([b"\x07\x00\x00\x00", u16_list([0])])
  assert get(dir, "BootOrder") == bytes.concat([b"\x07\x00\x00\x00", u16_list([1, 0, 2])])
  let none = invoke(ctx, dir, ["-r"])
  assert none.stdout == "No DriverOrder is set\n"
  let both = invoke(ctx, dir, ["-r", "-y"])
  assert both.status == 25
  assert invoke(ctx, dir, ["-r", "-n", "0000"]).status == 26
  assert invoke(ctx, dir, ["-y", "-t", "3"]).status == 27
}

const ESP_GUID = "11112222-3333-4444-5555-666677778888"

# A 4 MiB image with an ESP and a root partition, in the requested table.
proc disk_image(ctx: TestContext, name: Str, label: Str) [fs, process, error] -> Path {
  let image = test.temp_path(ctx, name: name)
  image.write(b"")?
  image.truncate(4194304)?
  let esp = if label == "gpt" { "c12a7328-f81f-11d2-ba4b-00a0c93ec93b" } else { "ef" }
  let data = if label == "gpt" { "0fc63daf-8483-4772-8e79-3d69d8477de4" } else { "83" }
  let uuids = if label == "gpt" { [ESP_GUID, "99998888-7777-6666-5555-444433332222"] } else { ["", ""] }
  let id = if label == "gpt" { "" } else { "0xdeadbeef" }
  linux.write_partition_table(image, {label: label, id: id, sector_size: 512, partitions: [{index: 1, start: 2048, end: 6143, size: 4096, type: esp, uuid: uuids[0], name: ""}, {index: 2, start: 6144, end: 8000, size: 1857, type: data, uuid: uuids[1], name: ""}]})?
  image
}

proc names_in(dir: Path) [fs, error] -> List[Str] {
  var names: List[Str] = []
  for child in fs.children(dir)? { names += [child.name] }
  names
}

# Whole-directory snapshot used to prove a refused command changed nothing.
proc snapshot(dir: Path) [fs, error] -> Str {
  var lines: List[Str] = []
  for child in fs.children(dir)? { lines += [f"{child.name} {child.path.read_bytes()?.base64()}"] }
  lines.join("\n")
}

test test_efibootmgr_create_builds_hard_drive_and_file_path_entry { |ctx|
  let dir = fixture(ctx, "efibootmgr-create")
  let image = disk_image(ctx, "efibootmgr-create.img", "gpt")
  let created = invoke(ctx, dir, ["-c", "-d", image.display(), "-p", "1", "-L", "Test Entry", "-l", "/EFI/test/x.efi"])
  assert created.status == 0, created.stderr
  let nodes = bytes.concat([hard_drive(1, 2048, 4096, [34, 34, 17, 17, 51, 51, 68, 68, 85, 85, 102, 102, 119, 119, 136, 136], 2), node(4, 4, utf16z("\\EFI\\test\\x.efi"))])
  assert get(dir, "Boot0003") == bytes.concat([b"\x07\x00\x00\x00", load_option(1, "Test Entry", nodes, b"")])
  assert get(dir, "BootOrder") == bytes.concat([b"\x07\x00\x00\x00", u16_list([3, 1, 0, 2])])
  assert "BootOrder: 0003,0001,0000,0002\n" in created.stdout
  assert f"Boot0003* Test Entry\tHD(1,GPT,{ESP_GUID},0x800,0x1000)/\\EFI\\test\\x.efi\n" in created.stdout
}

test test_efibootmgr_create_honors_index_create_only_and_explicit_number { |ctx|
  let dir = fixture(ctx, "efibootmgr-create-index")
  let image = disk_image(ctx, "efibootmgr-create-index.img", "gpt")
  let indexed = invoke(ctx, dir, ["-c", "-d", image.display(), "-I", "1", "-L", "Second"])
  assert indexed.status == 0, indexed.stderr
  assert get(dir, "BootOrder") == bytes.concat([b"\x07\x00\x00\x00", u16_list([1, 3, 0, 2])])
  let only = invoke(ctx, dir, ["-C", "-b", "0010", "-d", image.display(), "-p", "2", "-L", "Detached"])
  assert only.status == 0, only.stderr
  assert exists(dir, "Boot0010")
  assert get(dir, "BootOrder") == bytes.concat([b"\x07\x00\x00\x00", u16_list([1, 3, 0, 2])])
  assert "Boot0010* Detached\tHD(2,GPT,99998888-7777-6666-5555-444433332222,0x1800,0x741)/\\EFI\\BOOT\\BOOTX64.EFI\n" in only.stdout
  let clash = invoke(ctx, dir, ["-c", "-b", "0010", "-d", image.display()])
  assert clash.status == 40
  assert "Cannot create Boot0010: already exists." in clash.stderr
  let beyond = invoke(ctx, dir, ["-c", "-I", "99", "-d", image.display(), "-L", "Last"])
  assert beyond.status == 0, beyond.stderr
  assert get(dir, "BootOrder") == bytes.concat([b"\x07\x00\x00\x00", u16_list([1, 3, 0, 2, 4])])
  assert invoke(ctx, dir, ["-I", "1"]).status == 1
}

test test_efibootmgr_create_warns_about_duplicate_labels_and_fills_a_missing_order { |ctx|
  let dir = test.temp_dir(ctx, name: "efibootmgr-create-empty")?
  let image = disk_image(ctx, "efibootmgr-create-empty.img", "gpt")
  let first = invoke(ctx, dir, ["-c", "-d", image.display()])
  assert first.status == 0, first.stderr
  assert get(dir, "BootOrder") == bytes.concat([b"\x07\x00\x00\x00", u16_list([0])])
  assert "Boot0000* Linux\t" in first.stdout
  let second = invoke(ctx, dir, ["-c", "-d", image.display()])
  assert second.status == 0, second.stderr
  assert second.stderr == "efibootmgr: ** Warning ** : Boot0000 has same label Linux\n"
  assert get(dir, "BootOrder") == bytes.concat([b"\x07\x00\x00\x00", u16_list([1, 0])])
}

test test_efibootmgr_create_dos_table_uses_signature_node { |ctx|
  let dir = test.temp_dir(ctx, name: "efibootmgr-create-dos")?
  let image = disk_image(ctx, "efibootmgr-create-dos.img", "dos")
  let created = invoke(ctx, dir, ["-c", "-d", image.display(), "-p", "2"])
  assert created.status == 0, created.stderr
  assert "Boot0000* Linux\tHD(2,MBR,0xdeadbeef,0x1800,0x741)/\\EFI\\BOOT\\BOOTX64.EFI\n" in created.stdout
}

test test_efibootmgr_create_file_dev_path_and_optional_data { |ctx|
  let dir = test.temp_dir(ctx, name: "efibootmgr-create-data")?
  let plain = invoke(ctx, dir, ["-c", "--file-dev-path", "-l", "\\shim.efi", "root=/dev/sda2", "quiet"])
  assert plain.status == 0, plain.stderr
  assert get(dir, "Boot0000") == bytes.concat([b"\x07\x00\x00\x00", load_option(1, "Linux", node(4, 4, utf16z("\\shim.efi")), b"root=/dev/sda2 quiet")])
  let wide = invoke(ctx, dir, ["-c", "-u", "--file-dev-path", "-l", "\\a.efi", "ab"])
  assert wide.status == 0, wide.stderr
  assert get(dir, "Boot0001") == bytes.concat([b"\x07\x00\x00\x00", load_option(1, "Linux", node(4, 4, utf16z("\\a.efi")), b"a\x00b\x00")])
  let raw = invoke_with(ctx, dir, ["-c", "--file-dev-path", "-@", "-"], false, b"\x01\x02")
  assert raw.status == 0, raw.stderr
  assert get(dir, "Boot0002") == bytes.concat([b"\x07\x00\x00\x00", load_option(1, "Linux", node(4, 4, utf16z("\\EFI\\BOOT\\BOOTX64.EFI")), b"\x01\x02")])
}

test test_efibootmgr_failed_create_leaves_the_directory_untouched { |ctx|
  let dir = fixture(ctx, "efibootmgr-create-fail")
  let before = snapshot(dir)
  let image = disk_image(ctx, "efibootmgr-create-fail.img", "gpt")
  let missing_disk = invoke(ctx, dir, ["-c", "-d", fp"{dir}/absent.img".display()])
  assert missing_disk.status == 5
  assert missing_disk.stderr == "efibootmgr: ** Warning ** : Boot0000 has same label Linux\nCould not prepare Boot variable: No such file or directory\n"
  let missing_part = invoke(ctx, dir, ["-c", "-d", image.display(), "-p", "7", "-L", "Unique"])
  assert missing_part.status == 5
  assert "partition 7 not found" in missing_part.stderr
  assert snapshot(dir) == before
}

test test_efibootmgr_refuses_directories_that_are_not_efivarfs_shaped { |ctx|
  let dir = fixture(ctx, "efibootmgr-shape")
  fp"{dir}/notes.txt".write("keep me")
  let before = snapshot(dir)
  let refused = invoke(ctx, dir, ["-t", "9"])
  assert refused.status == 2
  assert "is not an efivarfs directory: unexpected entry notes.txt" in refused.stderr
  assert snapshot(dir) == before
  let listing = invoke(ctx, dir, [])
  assert listing.status == 2
  let allowed = invoke_with(ctx, dir, ["-t", "9"], true, b"")
  assert allowed.status == 0, allowed.stderr
  assert get(dir, "Timeout") == bytes.concat([b"\x07\x00\x00\x00", u16(9)])
  assert fp"{dir}/notes.txt".read_text()? == "keep me"
  let absent = invoke(ctx, fp"{dir}/nowhere", [])
  assert absent.status == 2
  assert absent.stderr == "EFI variables are not supported on this system.\n"
}

test test_efibootmgr_writes_only_named_variables_in_the_directory { |ctx|
  let dir = fixture(ctx, "efibootmgr-contained")
  let image = disk_image(ctx, "efibootmgr-contained.img", "gpt")
  let before = names_in(dir)
  let result = invoke(ctx, dir, ["-c", "-d", image.display(), "-n", "0000", "-t", "2"])
  assert result.status == 0, result.stderr
  let after = names_in(dir)
  var added: List[Str] = []
  for name in after { if name not in before { added += [name] } }
  let ordered = added |> sort-by .
  assert ordered == [f"Boot0003{GUID_SUFFIX}", f"BootNext{GUID_SUFFIX}"], added.join(",")
}

test test_efibootmgr_clears_and_restores_the_immutable_flag_around_writes { |ctx|
  let dir = fixture(ctx, "efibootmgr-immutable")
  let log = test.temp_path(ctx, name: "efibootmgr-immutable-log")
  test.linux_fake(ctx, {file_attrs_flags: 16, log: log})
  let written = invoke(ctx, dir, ["-t", "4"])
  assert written.status == 0, written.stderr
  let entries = log.read_text()?.lines()
  var flags: List[Str] = []
  for line in entries {
    if "set_file_attrs" in line {
      let record = json.decode(line)?
      flags += [record.flags.require(Str)?]
    }
  }
  assert flags == ["0", "16"], log.read_text()?
  log.write("")
  let cleared = invoke(ctx, dir, ["-T"])
  assert cleared.status == 0, cleared.stderr
  assert ! exists(dir, "Timeout")
  assert "\"flags\":\"0\"" in log.read_text()?
}

test test_efibootmgr_rejects_options_it_cannot_honor_by_name { |ctx|
  let dir = fixture(ctx, "efibootmgr-unsupported")
  let before = snapshot(dir)
  for option in ["-e", "-E", "-g", "-i", "-m", "-M", "-w", "--full-dev-path"] {
    let args = if option in ["-g", "-w", "--full-dev-path"] { [option] } else { [option, "1"] }
    let result = invoke(ctx, dir, args)
    assert result.status == 1, option
    assert "is not supported:" in result.stderr, option
    assert result.stdout == "", option
  }
  assert snapshot(dir) == before
}

test test_efibootmgr_rejects_options_that_would_do_nothing { |ctx|
  let dir = fixture(ctx, "efibootmgr-noop")
  let before = snapshot(dir)
  for args in [["-d", "/dev/sda"], ["-p", "2"], ["-l", "x.efi"], ["-L", "x"], ["-u"], ["--file-dev-path"], ["stray"]] {
    let result = invoke(ctx, dir, args)
    assert result.status == 1, args.join(" ")
    assert "efibootmgr: " in result.stderr, args.join(" ")
  }
  let usage = invoke(ctx, dir, ["--bogus"])
  assert usage.status == 1
  assert usage.stdout.starts_with("efibootmgr version 18\nusage: efibootmgr [options]\n")
  assert usage.stderr == "efibootmgr: unrecognized option: bogus\n"
  assert snapshot(dir) == before
}

test test_efibootmgr_numeric_arguments_report_the_offending_column { |ctx|
  let dir = fixture(ctx, "efibootmgr-numbers")
  let name = invoke(ctx, dir, ["-b", "00zz"])
  assert name.status == 28
  assert name.stderr == "Invalid bootnum value00zz\n                         ^\n"
  let range = invoke(ctx, dir, ["-b", "10000"])
  assert range.status == 29
  assert range.stderr == "Invalid bootnum value: 10000\n\n"
  let next = invoke(ctx, dir, ["-n", "zz"])
  assert next.status == 35
  let version = invoke(ctx, dir, ["-V"])
  assert version.stdout == "version 18\n"
  let quiet = invoke(ctx, dir, ["-q", "-t", "8"])
  assert quiet.status == 0
  assert quiet.stdout == ""
}
