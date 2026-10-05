use core.lib.sys_usb as usb
use core.lib.system_report as report

# Device descriptor, one configuration, one HID interface, and one interrupt endpoint.
pure hid_descriptors() -> Bytes {
  b"\x12\x01\0\x02\0\0\0@m\x04+\xc5\0\x01\x01\x02\x03\x01\t\x02\x19\0\x01\x01\0\xa02\t\x04\0\0\x01\x03\x01\x01\0\x07\x05\x81\x03\x08\0\n"
}

# Writes one device's identity attributes under `sys/devices/LOCATION` and links it
# from the bus listing; the caller creates `sys/bus/usb/devices` once.
proc add_device(root: FsRoot, name: Str, location: Str, vendor: Str, product: Str, number: Int) [fs, error] {
  root.mkdir(fp"sys/devices/{location}/power", parents: true)
  for item in [
    {file: "idVendor", value: vendor},
    {file: "idProduct", value: product},
    {file: "busnum", value: "1"},
    {file: "devnum", value: f"{number}"},
    {file: "bcdDevice", value: "0100"},
    {file: "bDeviceClass", value: "00"},
    {file: "bDeviceSubClass", value: "00"},
    {file: "bDeviceProtocol", value: "00"},
    {file: "manufacturer", value: "Acme"},
    {file: "product", value: f"Thing {name}"},
    {file: "speed", value: "480"},
    {file: "bNumConfigurations", value: "1"},
    {file: "bConfigurationValue", value: "1"},
    {file: "power/control", value: "auto"},
    {file: "power/autosuspend_delay_ms", value: "2000"},
    {file: "power/runtime_status", value: "active"},
  ] {
    root.write(fp"sys/devices/{location}/{item.file}", f"{item.value}\n")
  }

  root.symlink(fp"../../../devices/{location}", fp"sys/bus/usb/devices/{name}")
}

test test_sys_usb_descriptor_stream_keeps_framing_and_unknown_payloads {
  let records = usb.parse_descriptor_stream(b"\x03\x99B\x02\xfe")?
  assert records.len() == 2
  assert records[0].offset == 0 and records[0].length == 3 and records[0].descriptor_type == 153
  assert records[0].raw == b"\x03\x99B"
  assert records[1].offset == 3 and records[1].descriptor_type == 254
  for bad in [b"\x01\x02", b"\x04\x01x", b"\t", b"\x02"] {
    test.error_kind(usb.parse_descriptor_stream(bad), "SysUsbError.InvalidStream")
  }
}

test test_sys_usb_alternates_keep_configuration_and_endpoint_ownership {
  let alternates = usb.parse_alternates(hid_descriptors())?
  assert alternates.len() == 1
  let setting = alternates[0]
  assert setting.configuration_value == 1
  assert setting.interface_number == 0 and setting.setting_number == 0
  assert setting.class_code == 3 and setting.subclass == 1 and setting.protocol == 1
  assert setting.endpoints.len() == 1
  assert setting.endpoints[0].address == 129
  assert setting.endpoints[0].direction == "in"
  assert setting.endpoints[0].transfer_type == "interrupt"
  assert setting.endpoints[0].max_packet_size == 8
  assert setting.endpoints[0].interval == 10

  for bad in [
    b"\x07\x05\x81\x02@\0\0",
    b"\x04\x04\0\0",
    b"\t\x02\x08\0\x01\x01\0\x802",
    b"\t\x02\xff\xff\x01\x01\0\x802",
  ] {
    test.error_kind(usb.parse_alternates(bad), "SysUsbError.InvalidDescriptor")
  }
}

test test_sys_usb_collect_links_hub_children_controller_and_interfaces {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/bus/usb/devices", parents: true)
  let controller = "pci0000:00/0000:00:14.0"
  add_device(root, "usb1", f"{controller}/usb1", "1d6b", "0002", 1)
  add_device(root, "1-1", f"{controller}/usb1/1-1", "05e3", "0610", 2)
  add_device(root, "1-1.2", f"{controller}/usb1/1-1/1-1.2", "046d", "c52b", 3)
  let interface_path = f"{controller}/usb1/1-1/1-1.2/1-1.2:1.0"
  root.mkdir(fp"sys/devices/{interface_path}", parents: true)
  root.write(fp"sys/devices/{interface_path}/bAlternateSetting", "0\n")
  root.symlink(../../../../../../../../../bus/usb/drivers/usbhid, fp"sys/devices/{interface_path}/driver")
  root.symlink(fp"../../../devices/{interface_path}", p"sys/bus/usb/devices/1-1.2:1.0")
  root.write(fp"sys/devices/{controller}/usb1/1-1/1-1.2/descriptors", hid_descriptors())

  let inventory = usb.collect(root)
  assert inventory.listing_state == "complete"
  assert inventory.enumeration_succeeded
  assert inventory.issues == []
  assert inventory.devices.len() == 3, "interface entries are not devices"
  let by_name = {device.sysfs_name: device for device in inventory.devices}
  let hub_root = by_name.get("usb1")?
  let hub = by_name.get("1-1")?
  let leaf = by_name.get("1-1.2")?
  assert hub_root.is_root_hub and ! leaf.is_root_hub
  assert leaf.vendor_id == 1133 and leaf.product_id == 50475
  assert leaf.bus_number == 1 and leaf.device_number == 3
  assert leaf.device_version == "0100" and leaf.speed_mbps == "480"
  assert leaf.port_path == "1.2" and hub.port_path == "1"
  assert leaf.controller_pci_address == "0000:00:14.0", "the controller is the PCI function in the bus-entry link"
  assert leaf.autosuspend_delay_ms == 2000 and leaf.runtime_status == "active"
  assert inventory.devices[leaf.parent_device_index ?? -1].sysfs_name == "1-1"
  assert inventory.devices[hub.parent_device_index ?? -1].sysfs_name == "usb1"
  assert hub_root.parent_device_index == null
  assert leaf.interfaces.len() == 1
  assert leaf.interfaces[0].number == 0
  assert leaf.interfaces[0].driver == "usbhid"
  assert leaf.interfaces[0].active_alternate == 0
  assert leaf.interfaces[0].alternate_settings.len() == 1
  assert leaf.interfaces[0].alternate_settings[0].endpoints[0].transfer_type == "interrupt"
}

test test_sys_usb_collect_keeps_a_device_with_invalid_identity_and_reports_it {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/bus/usb/devices", parents: true)
  add_device(root, "1-1", "pci0000:00/0000:00:14.0/usb1/1-1", "zz", "0610", 2)
  add_device(root, "1-2", "pci0000:00/0000:00:14.0/usb1/1-2", "046d", "c52b", 3)
  root.write(p"sys/devices/pci0000:00/0000:00:14.0/usb1/1-2/descriptors", b"\x02\x02\0")

  let inventory = usb.collect(root)
  assert inventory.devices.len() == 2
  let damaged = (inventory.devices |> where .sysfs_name == "1-1")[0]
  assert damaged.vendor_id == null
  assert damaged.product_id == 1552
  let kinds = {item.field: item.error_kind for item in inventory.issues}
  assert kinds.get("devices.1-1.vendor_id")? == "invalid_usb_vendor_id"
  assert kinds.get("devices.1-2.descriptors")? == "invalid_usb_descriptor_stream"
}

test test_sys_usb_collect_reports_an_absent_bus_as_absent {
  let root = fs.tempdir()?
  defer root.close()?
  let inventory = usb.collect(root)
  assert inventory.listing_state == "absent"
  assert ! inventory.enumeration_succeeded
  assert inventory.devices == []
  assert inventory.issues[0].state == report.Absent
}

test test_sys_usb_parent_names_and_indexes_follow_sysfs_naming {
  assert usb.parent_name("1-1.2.3", 1) == "1-1.2"
  assert usb.parent_name("1-1", 1) == "usb1"
  assert usb.parent_name("usb1", 1) == null
  assert usb.parent_name("2-1", 1) == null, "a name must carry its own bus number"
  assert usb.parent_name("1-1", null) == null
  let names: List[Str?] = ["1-1.2", "usb1", "1-1", null]
  let buses: List[Int?] = [1, 1, 1, null]
  let expected: List[Int?] = [2, null, 1, null]
  assert usb.parent_indices(names, buses) == expected
  assert usb.name_indices(names).get("1-1")? == 2
}

test test_sys_usb_controller_address_distinguishes_links_directories_and_absence {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/devices/pci0000:00/0000:00:14.0/usb1", parents: true)
  root.mkdir(p"sys/bus/usb/devices", parents: true)
  root.symlink(../../../devices/pci0000:00/0000:00:14.0/usb1, p"sys/bus/usb/devices/usb1")
  root.mkdir(p"sys/bus/usb/devices/plain")

  let linked = usb.controller_address(root, p"sys/bus/usb/devices/usb1")
  assert linked.address == "0000:00:14.0" and linked.state == report.Observed
  let directory = usb.controller_address(root, p"sys/bus/usb/devices/plain")
  assert directory.address == null and directory.state == report.Observed
  let missing = usb.controller_address(root, p"sys/bus/usb/devices/gone")
  assert missing.address == null and missing.state == report.Disappeared
}
