use core.lib.btop_disks as disks
use core.lib.btop_model as model
use core.lib.btop_rates as rates
use core.lib.btop_source as source
use core.lib.sys_block as block
use core.lib.system_report as report

proc add_block_device(root: FsRoot, name: Str, number: Str, disk_sequence: Str, stats: Str) [fs, error] {
  let device_path = fp"sys/devices/virtual/block/{name}"
  root.mkdir(fp"{device_path}/holders", parents: true)
  root.mkdir(fp"{device_path}/slaves", parents: true)
  root.mkdir(fp"{device_path}/queue", parents: true)
  root.write(fp"{device_path}/dev", f"{number}\n")
  root.write(fp"{device_path}/diskseq", f"{disk_sequence}\n")
  root.write(fp"{device_path}/size", "4096\n")
  root.write(fp"{device_path}/stat", stats)
  root.symlink(fp"../../devices/virtual/block/{name}", fp"sys/class/block/{name}")
}

proc write_mounts(root: FsRoot, text: Str) [fs, error] {
  root.mkdir(p"proc/self", parents: true)
  root.write(p"proc/self/mountinfo", text)
}

proc frame(root: FsRoot, read_sectors: Int, write_sectors: Int, in_flight: Int, disk_sequence: Int) [fs, error] -> disks.Source {
  root.mkdir(p"sys/class/block", parents: true)
  add_block_device(
    root,
    "dm-0",
    "253:0",
    f"{disk_sequence}",
    f"1 0 {read_sectors} 0 1 0 {write_sectors} 0 {in_flight} 0 0\n",
  )
  root.mkdir(p"mnt/a b", parents: true)
  root.mkdir(p"mnt/alias", parents: true)
  write_mounts(
    root,
    """1 0 253:0 / /mnt/a\\040b rw - ext4 /dev/dm-0 rw
2 1 253:0 /sub /mnt/alias rw - ext4 /dev/dm-0 rw
""",
  )
  disks.collect(root, 1000000000)?
}

test test_btop_disk_rates_use_512_byte_accounting_sectors_and_start_warming {
  let first_root = fs.tempdir()?
  defer first_root.close()
  let first = frame(first_root, 100, 200, 7, 31)
  let initial = disks.step(first, null, 1000000000, null)
  assert initial.devices.len() == 1
  assert initial.devices[0].identity == "31:253:0"
  assert initial.devices[0].read_state == report.Observed
  assert initial.devices[0].read_bytes_per_second.state == rates.Warming
  assert initial.devices[0].write_bytes_per_second.state == rates.Warming

  let second_root = fs.tempdir()?
  defer second_root.close()
  let second = frame(second_root, 104, 205, 2, 31)
  let sample = disks.step(second, first, 2000000000, 1000000000)
  assert sample.devices.len() == 1, "two mount targets sharing a device are one disk row"
  assert sample.devices[0].read_bytes_per_second.value == 2048.0
  assert sample.devices[0].write_bytes_per_second.value == 2560.0
  assert sample.devices[0].in_flight == 2
  assert sample.devices[0].mount_targets == ["/mnt/a b", "/mnt/alias"]
  assert sample.devices[0].capacity_state == report.Observed
}

test test_btop_disk_device_replacement_and_counter_reset_do_not_reuse_rates {
  let first_root = fs.tempdir()?
  defer first_root.close()
  let first = frame(first_root, 100, 200, 1, 31)

  let replacement_root = fs.tempdir()?
  defer replacement_root.close()
  let replacement = frame(replacement_root, 500, 600, 3, 32)
  let replaced = disks.step(replacement, first, 2000000000, 1000000000)
  assert replaced.devices[0].read_bytes_per_second.state == rates.Warming

  replacement_root.remove(p"sys/devices/virtual/block/dm-0/diskseq")
  let missing_identity = disks.collect(replacement_root, 2000000000)?
  let without_identity = disks.step(missing_identity, first, 2000000000, 1000000000)
  assert without_identity.devices[0].identity_state == report.Absent
  assert without_identity.devices[0].read_bytes_per_second.state == rates.Unavailable

  let gap = disks.step(first, first, 2000000000, 1000000000, gap: true)
  assert gap.devices[0].read_bytes_per_second.state == rates.Gap
  let invalid_time = disks.step(first, first, 1000000000, 1000000000)
  assert invalid_time.devices[0].read_bytes_per_second.state == rates.Invalid

  let reset_root = fs.tempdir()?
  defer reset_root.close()
  let reset = frame(reset_root, 50, 75, 0, 31)
  let reset_panel = disks.step(reset, first, 2000000000, 1000000000)
  assert reset_panel.devices[0].read_bytes_per_second.state == rates.Reset
  assert reset_panel.devices[0].write_bytes_per_second.state == rates.Reset
  assert reset_panel.devices[0].in_flight == 0, "an observed zero gauge is not absent"
}

test test_btop_model_carries_disk_rates_and_captured_capacity {
  let first_root = fs.tempdir()?
  defer first_root.close()
  let second_root = fs.tempdir()?
  defer second_root.close()
  let first_fixture = frame(first_root, 100, 200, 7, 31)
  let second_fixture = frame(second_root, 104, 205, 2, 31)
  let first = source.capture(first_root, "disk-model", 1000000000, 4096, 100)?
  second_root.write(p"proc/self/mountinfo", "1 0 253:0 / /mnt/changed rw - ext4 /dev/dm-0 rw\n")
  let second = source.capture(
    second_root,
    "disk-model",
    2000000000,
    4096,
    100,
    cached_mounts: first.storage.mounts,
    cached_mounts_at_ns: first.storage.mounts_observed_at_ns,
  )?
  let initial = model.step(first, null)
  let current = model.step(second, first)
  assert first_fixture.blocks.devices.len() == 1 and second_fixture.mounts.mounts.len() == 2
  assert initial.storage.devices[0].read_bytes_per_second.state == rates.Warming
  assert current.storage.devices.len() == 1
  assert current.storage.devices[0].read_bytes_per_second.value == 2048.0
  assert current.storage.devices[0].write_bytes_per_second.value == 2560.0
  assert current.storage.devices[0].in_flight == 2
  assert current.storage.devices[0].mount_targets == ["/mnt/a b", "/mnt/alias"]
  assert current.storage.devices[0].capacity_age_ns == 1000000000
  assert second.storage.blocks.devices[0].io_counters |> any .name == "read_sectors" and .value == 104
  let wire = model.encode_wire_json(current)?
  assert "\"read_sectors\":\"104\"" in wire
}

test test_btop_disk_rows_use_only_the_mounted_stacked_device_counters {
  let root = fs.tempdir()?
  defer root.close()
  root.mkdir(p"sys/class/block", parents: true)
  add_block_device(root, "sda", "8:0", "11", "1 0 1000 0 1 0 2000 0 0 0 0\n")
  add_block_device(root, "dm-0", "253:0", "12", "1 0 10 0 1 0 20 0 0 0 0\n")
  root.symlink(p"../../sda", p"sys/devices/virtual/block/dm-0/slaves/sda")
  root.mkdir(p"mnt/root", parents: true)
  write_mounts(root, "1 0 253:0 / /mnt/root rw - ext4 /dev/dm-0 rw\n")
  let source = disks.collect(root, 1000000000)?
  let panel = disks.step(source, null, 1000000000, null)
  assert panel.devices.len() == 1
  assert panel.devices[0].name == "dm-0"
  assert panel.devices[0].read_sectors == 10
  assert panel.devices[0].read_bytes_per_second.state == rates.Warming
  assert block.index_of_number(block.index_by_number(source.blocks.devices), 8, 0) != null
}

test test_btop_disk_counter_byte_conversion_withholds_sector_delta_overflow {
  let first_root = fs.tempdir()?
  defer first_root.close()
  let first = frame(first_root, 0, 0, 1, 31)

  let second_root = fs.tempdir()?
  defer second_root.close()
  let second = frame(second_root, 9223372036854775807, 9223372036854775807, 1, 31)
  let sample = disks.step(second, first, 2000000000, 1000000000)
  assert sample.devices[0].read_bytes_per_second.state == rates.Invalid
  assert sample.devices[0].write_bytes_per_second.state == rates.Invalid
}

test test_btop_disk_invalid_stat_layout_keeps_malformed_separate_from_absent {
  let root = fs.tempdir()?
  defer root.close()
  let _ = frame(root, 10, 20, 1, 31)
  root.write(p"sys/devices/virtual/block/dm-0/stat", "1 0 10 0 1 0 20 0 1 0 0 5 6\n")
  let source = disks.collect(root, 1000000000)?
  let panel = disks.step(source, null, 1000000000, null)
  assert panel.devices[0].read_state == report.Malformed
  assert panel.devices[0].read_sectors == null
  assert panel.devices[0].read_bytes_per_second.state == rates.Unavailable
}
