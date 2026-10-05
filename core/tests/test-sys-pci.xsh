use core.lib.sys_pci as pci
use core.lib.system_report as report

# Writes one function's identity attributes under its real sysfs device path and
# links it from the bus listing the way the kernel does. The caller creates
# `sys/bus/pci/devices` once.
proc add_function(root: FsRoot, address: Str, device_path: Str, class_code: Str) [fs, error] {
  root.mkdir(fp"sys/devices/{device_path}", parents: true)
  for item in [
    {name: "vendor", value: "0x144d"},
    {name: "device", value: "0xa808"},
    {name: "subsystem_vendor", value: "0x144d"},
    {name: "subsystem_device", value: "0x0001"},
    {name: "class", value: class_code},
    {name: "revision", value: "0x01"},
  ] {
    root.write(fp"sys/devices/{device_path}/{item.name}", f"{item.value}\n")
  }

  root.symlink(fp"../../../devices/{device_path}", fp"sys/bus/pci/devices/{address}")
}

test test_sys_pci_address_and_identifier_parsers_keep_typed_boundaries {
  assert pci.parse_address("0001:af:1f.7")? == {domain: 1, bus: 175, device: 31, function: 7}
  for bad in ["0000:00:20.0", "0000:00:01.8", "00:00:01.0", "0000:0g:01.0", "0000:00:01"] {
    test.error_kind(pci.parse_address(bad), "SysPciError.InvalidAddress")
  }

  assert pci.parse_hex_value("0x10DE")? == 4318
  assert pci.parse_hex_value("10de")? == 4318
  for bad in ["0x10xz", "", "0x1ffffffff"] {
    test.error_kind(pci.parse_hex_value(bad), "SysPciError.InvalidId")
  }

  assert pci.parse_decimal_value("9007199254740991")? == 9007199254740991
  for bad in ["", "0x8", "-1", "9007199254740992"] {
    test.error_kind(pci.parse_decimal_value(bad), "SysPciError.InvalidId")
  }
}

test test_sys_pci_collect_links_a_function_to_its_bridge_by_index {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/bus/pci/devices", parents: true)
  add_function(root, "0001:02:01.0", "pci0001:02/0001:02:01.0", "0x060400")
  add_function(root, "0001:02:03.0", "pci0001:02/0001:02:01.0/0001:02:03.0", "0x010802")
  root.write(p"sys/devices/pci0001:02/0001:02:01.0/0001:02:03.0/numa_node", "-1\n")
  root.write(p"sys/devices/pci0001:02/0001:02:01.0/0001:02:03.0/max_link_width", "4\n")
  root.mkdir(p"sys/bus/pci/drivers/nvme", parents: true)
  root.symlink(
    ../../../../../../bus/pci/drivers/nvme,
    p"sys/devices/pci0001:02/0001:02:01.0/0001:02:03.0/driver",
  )

  let inventory = pci.collect(root)
  assert inventory.status.state == report.Complete
  assert inventory.status.enumeration_succeeded
  assert inventory.issues == []
  assert inventory.functions.len() == 2
  let bridge = (inventory.functions |> where .address == "0001:02:01.0")[0]
  let child = (inventory.functions |> where .address == "0001:02:03.0")[0]
  assert child.domain == 1 and child.bus == 2 and child.device == 3 and child.function == 0
  assert child.vendor_id == 5197 and child.class_code == 67586 and child.revision == 1
  assert child.driver == "nvme"
  assert child.numa_node == null, "-1 means the kernel does not know the node"
  assert child.maximum_link_width == 4
  assert bridge.parent_function_index == null
  assert inventory.functions[child.parent_function_index ?? -1].address == "0001:02:01.0"
  assert report.pci_parent_function(inventory.functions, child)?.address == "0001:02:01.0"
}

test test_sys_pci_collect_keeps_valid_neighbors_of_malformed_functions {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/bus/pci/devices", parents: true)
  add_function(root, "0000:00:02.0", "pci0000:00/0000:00:02.0", "0x030000")
  add_function(root, "0000:00:03.0", "pci0000:00/0000:00:03.0", "0x030000")
  root.write(p"sys/devices/pci0000:00/0000:00:03.0/revision", "not-hex\n")
  root.mkdir(p"sys/bus/pci/devices/zzzz:bad", parents: true)

  let inventory = pci.collect(root)
  assert inventory.status.state == report.Partial
  assert inventory.functions.len() == 2
  let damaged = (inventory.functions |> where .address == "0000:00:03.0")[0]
  assert damaged.revision == null
  assert damaged.vendor_id == 5197
  let fields = [item.field for item in inventory.issues]
  assert "functions.zzzz:bad" in fields
  assert "functions.0000:00:03.0.revision" in fields
  assert (inventory.issues |> where .field == "functions.0000:00:03.0.revision")[0].error_kind == "invalid_integer"
}

test test_sys_pci_collect_reports_an_absent_bus_as_absent_not_empty_success {
  let root = fs.tempdir()?
  defer root.close()?
  let inventory = pci.collect(root)
  assert inventory.status.state == report.SectionAbsent
  assert ! inventory.status.enumeration_succeeded
  assert inventory.functions == []
  assert inventory.issues.len() == 1
  assert inventory.issues[0].field == "functions"
}

test test_sys_pci_path_helpers_resolve_addresses_and_indexes {
  let target = ../../../devices/pci0000:00/0000:00:08.1/0000:04:00.4/usb4/4-2
  assert pci.address_in_target(target) == "0000:04:00.4"
  assert pci.address_in_target(../../../devices/platform/soc/usb1/1-2) == null
  assert pci.parent_bridge_address(../../../devices/pci0001:02/0001:02:01.0/0001:02:03.0, "0001:02:03.0") == "0001:02:01.0"
  assert pci.parent_bridge_address(../../../devices/pci0001:02/0001:02:03.0, "0001:02:03.0") == null

  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/bus/pci/devices", parents: true)
  add_function(root, "0000:00:02.0", "pci0000:00/0000:00:02.0", "0x030000")
  let indices = pci.function_indices(pci.collect(root).functions)
  assert pci.function_index(indices, "0000:00:02.0") == 0
  assert pci.function_index(indices, "0000:00:09.0") == null
  assert pci.function_index(indices, null) == null
}
