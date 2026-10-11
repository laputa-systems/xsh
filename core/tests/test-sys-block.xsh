use core.lib.sys_block as block
use core.lib.system_report as report

# Creates a class entry for a device whose real directory lives at `device_path`.
# Partitions expose holders but no slaves directory of their own.
proc add_device(root: FsRoot, name: Str, device_path: Str, number: Str, sectors: Str, slaves: Bool = true) [fs, error] {
  root.mkdir(fp"sys/devices/{device_path}/holders", parents: true)
  if slaves {
    root.mkdir(fp"sys/devices/{device_path}/slaves", parents: true)
  }

  root.mkdir(fp"sys/devices/{device_path}/queue", parents: true)
  root.write(fp"sys/devices/{device_path}/dev", f"{number}\n")
  root.write(fp"sys/devices/{device_path}/size", f"{sectors}\n")
  root.write(fp"sys/devices/{device_path}/queue/logical_block_size", "512\n")
  root.write(fp"sys/devices/{device_path}/queue/physical_block_size", "4096\n")
  root.write(fp"sys/devices/{device_path}/queue/rotational", "0\n")
  root.write(fp"sys/devices/{device_path}/removable", "0\n")
  root.write(fp"sys/devices/{device_path}/ro", "1\n")
  root.symlink(target: fp"../../devices/{device_path}", path: fp"sys/class/block/{name}")
}

test test_sys_block_scheduler_parser_requires_exactly_one_selected_choice {
  let selected = block.parse_scheduler("none [mq-deadline] kyber").require(block.BlockScheduler)?
  assert selected.active == "mq-deadline"
  assert selected.available == ["none", "mq-deadline", "kyber"]
  assert block.parse_scheduler("[none]\tfixture-scheduler").require(block.BlockScheduler)?.available == [
    "none",
    "fixture-scheduler",
  ]
  for invalid in ["", "none kyber", "[none] [kyber]", "[none] none", "[]", "a\nb"] {
    assert block.parse_scheduler(invalid) == null, invalid
  }
}

test test_sys_block_collect_links_partitions_stacked_devices_and_pci_parent {
  let root = fs.tempdir()?
  defer root.close()
  root.mkdir(p"sys/class/block", parents: true)
  let disk = "pci0000:00/0000:00:01.2/0000:01:00.0/nvme/nvme0/nvme0n1"
  add_device(root, "nvme0n1", disk, "259:0", "2048")
  add_device(root, "nvme0n1p2", f"{disk}/nvme0n1p2", "259:2", "1024", slaves: false)
  root.write(fp"sys/devices/{disk}/nvme0n1p2/partition", "2\n")
  root.mkdir(fp"sys/devices/{disk}/device", parents: true)
  root.write(fp"sys/devices/{disk}/device/model", "Fixture NVMe\n")
  root.write(fp"sys/devices/{disk}/device/rev", "REV9\n")
  root.write(fp"sys/devices/{disk}/queue/scheduler", "[none] mq-deadline\n")
  root.write(fp"sys/devices/{disk}/stat", "1 2 3 4 5 6 7 8 9 10 11\n")
  add_device(root, "dm-0", "virtual/block/dm-0", "253:0", "512")
  root.symlink(target: ../../../../../../../../virtual/block/dm-0, path: fp"sys/devices/{disk}/nvme0n1p2/holders/dm-0")
  root.symlink(target: fp"../../../../{disk}/nvme0n1p2", path: p"sys/devices/virtual/block/dm-0/slaves/nvme0n1p2")

  let inventory = block.collect(root)
  assert inventory.listing_state == "complete"
  assert inventory.enumeration_succeeded
  assert inventory.issues == [], "no field issue is expected for a complete fixture"
  let names = [device.name for device in inventory.devices]
  assert (names |> sort) == ["dm-0", "nvme0n1", "nvme0n1p2"]
  let by_name = {device.name: device for device in inventory.devices}
  let whole = by_name.get("nvme0n1")?
  let part = by_name.get("nvme0n1p2")?
  let stacked = by_name.get("dm-0")?
  assert whole.kind == "disk" and part.kind == "partition" and stacked.kind == "virtual"
  assert whole.major == 259 and whole.minor == 0 and part.minor == 2
  assert whole.size_bytes == 1048576 and whole.logical_sector_bytes == 512 and whole.physical_sector_bytes == 4096
  assert whole.read_only == true and whole.rotational == false and whole.removable == false
  assert whole.active_scheduler == "none" and whole.available_schedulers == ["none", "mq-deadline"]
  assert whole.parent_pci_address == "0000:01:00.0"
  assert whole.model.value == "Fixture NVMe"
  assert whole.firmware.value == "REV9", "firmware falls back to the legacy rev attribute"
  assert ([item.name for item in whole.io_counters] |> take(2)) == ["read_ios", "read_merges"]
  assert whole.io_counters.len() == 11
  assert part.parent_name == "nvme0n1"
  assert inventory.devices[part.parent_device_index ?? -1].name == "nvme0n1"
  assert part.holder_names == ["dm-0"]
  assert inventory.devices[part.holder_indices[0]].name == "dm-0"
  assert stacked.slave_names == ["nvme0n1p2"]
  assert inventory.devices[stacked.slave_indices[0]].name == "nvme0n1p2"
  assert stacked.parent_pci_address == null

  let numbers = block.index_by_number(inventory.devices)
  assert inventory.devices[block.index_of_number(numbers, 259, 2) ?? -1].name == "nvme0n1p2"
  assert block.index_of_number(numbers, 8, 0) == null
}

test test_sys_block_collect_keeps_a_device_with_missing_or_invalid_fields {
  let root = fs.tempdir()?
  defer root.close()
  root.mkdir(p"sys/class/block", parents: true)
  add_device(root, "sda", "pci0000:00/0000:00:1f.2/ata1/block/sda", "8:0", "16")
  add_device(root, "sdb", "pci0000:00/0000:00:1f.2/ata2/block/sdb", "8:16", "16")
  root.write(p"sys/devices/pci0000:00/0000:00:1f.2/ata2/block/sdb/dev", "eight\n")
  root.write(p"sys/devices/pci0000:00/0000:00:1f.2/ata2/block/sdb/removable", "2\n")
  root.write(p"sys/devices/pci0000:00/0000:00:1f.2/ata2/block/sdb/queue/scheduler", "none kyber\n")

  let inventory = block.collect(root)
  assert inventory.devices.len() == 2
  let damaged = (inventory.devices |> where .name == "sdb")[0]
  assert damaged.major == null and damaged.minor == null
  assert damaged.removable == null
  assert damaged.active_scheduler == null
  let fields = [item.field for item in inventory.issues]
  assert "devices.sdb.major_minor" in fields
  assert "devices.sdb.removable" in fields
  assert "devices.sdb.scheduler" in fields
  assert ! ("devices.sda.major_minor" in fields)
  assert (inventory.devices |> where .name == "sda")[0].major == 8
}

test test_sys_block_collect_reports_an_absent_class_directory_as_absent {
  let root = fs.tempdir()?
  defer root.close()
  let inventory = block.collect(root)
  assert inventory.listing_state == "absent"
  assert ! inventory.enumeration_succeeded
  assert inventory.devices == []
  assert inventory.issues.len() == 1
  assert inventory.issues[0].state == .Absent
}
