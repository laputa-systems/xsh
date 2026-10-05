use system_report_lsusb_check as lsusb_reference

test test_system_report_lsusb_saved_outputs_score_ids_tree_and_selected_descriptor {
  let devices = lsusb_reference.parse_lsusb_list("""Bus 001 Device 001: ID 1d6b:0002 Linux root hub
Bus 001 Device 002: ID 05e3:0610 USB hub
""")?
  let tree = lsusb_reference.parse_lsusb_tree("""/:  Bus 001.Port 001: Dev 001, Class=root_hub, Driver=xhci_hcd/2p, 480M
    |__ Port 002: Dev 002, If 0, Class=Hub, Driver=hub/4p, 480M
""")?
  let descriptor = lsusb_reference.parse_lsusb_verbose(
    """
Bus 001 Device 001: ID 1d6b:0002
Device Descriptor:
  idVendor 0x1d6b Linux
  idProduct 0x0002 Hub
  bDeviceClass 9
  bNumConfigurations 1
""",
    1,
    1,
  )?
  let candidate = """{"usb":{"devices":[{"bus_number":1,"device_number":1,"vendor_id":7531,"product_id":2,"class_code":9,"configuration_count":1,"port_path":null,"interfaces":[]},{"bus_number":1,"device_number":2,"vendor_id":1507,"product_id":1552,"class_code":9,"configuration_count":1,"port_path":"2","interfaces":[{"number":0,"driver":"hub"}]}]}}"""
  let exact = lsusb_reference.compare_lsusb(candidate, devices, tree, descriptor)?
  assert exact.matched_devices == 2
  assert exact.matched_tree_rows == 2
  assert exact.matched_descriptor_fields == 4
  assert exact.mismatches.len() == 0 and exact.partial.len() == 0
  let wrong = lsusb_reference.compare_lsusb(
    candidate.replace("\"driver\":\"hub\"", "\"driver\":\"wrong\""),
    devices,
    tree,
    descriptor,
  )?
  assert "1:2.driver" in wrong.mismatches
  let wrong_port = lsusb_reference.compare_lsusb(
    candidate.replace("\"port_path\":\"2\"", "\"port_path\":\"3\""),
    devices,
    tree,
    descriptor,
  )?
  assert "1:2.port" in wrong_port.mismatches
}

test test_system_report_lsusb_rejects_duplicate_and_unsupported_utility_rows {
  test.error_kind(
    lsusb_reference.parse_lsusb_list("""Bus 001 Device 001: ID 1d6b:0002
Bus 001 Device 001: ID 1d6b:0002
"""),
    "LsusbCheckError.Invalid",
  )
  test.error_kind(
    lsusb_reference.parse_lsusb_list("""Bus 001 Device 001: ID 1d6b:xyz2
"""),
    "LsusbCheckError.Invalid",
  )
  test.error_kind(
    lsusb_reference.parse_lsusb_tree("""|__ Port 002: Dev 002, If 0, Driver=hub/4p
"""),
    "LsusbCheckError.Invalid",
  )
  test.error_kind(
    lsusb_reference.parse_lsusb_verbose(
  """idVendor 0x1d6b
""",
  1,
  1,
),
    "LsusbCheckError.Invalid",
  )
  test.error_kind(
    lsusb_reference.parse_lsusb_verbose(
  """Bus 002 Device 001: ID 1d6b:0002
idVendor 0x1d6b
idProduct 0x0002
bDeviceClass 9
bNumConfigurations 1
""",
  1,
  1,
),
    "LsusbCheckError.Invalid",
  )
}

test test_system_report_lsusb_live_reference_uses_bounded_selected_descriptor {
  let tools_root = fs.tempdir()?
  defer tools_root.close()?
  tools_root.write(
    p"lsusb",
    """#!/bin/sh
case "$*" in
  "--version") printf 'lsusb (usbutils) 019\n' ;;
  "") printf 'Bus 001 Device 001: ID 1d6b:0002 Root hub\n' ;;
  "-t") printf '/:  Bus 001.Port 001: Dev 001, Class=root_hub, Driver=xhci_hcd/2p, 480M\n' ;;
  "-v -s 1:1") printf '\nBus 001 Device 001: ID 1d6b:0002\nDevice Descriptor:\n  idVendor 0x1d6b\n  idProduct 0x0002\n  bDeviceClass 9\n  bNumConfigurations 1\n' ;;
  *) exit 4 ;;
esac
""",
  )
  tools_root.write(
    p"xsh",
    """#!/bin/sh
printf '{"source_mode":"live_linux","usb":{"devices":[{"bus_number":1,"device_number":1,"vendor_id":7531,"product_id":2,"class_code":9,"configuration_count":1,"port_path":null,"interfaces":[]}]}}\n'
""",
  )
  tools_root.chmod(p"lsusb", 0o700)
  tools_root.chmod(p"xsh", 0o700)
  let root_path = tools_root.host_path()?
  let result = lsusb_reference.compare_live_lsusb(
    fp"{root_path}/xsh".display(),
    fp"{root_path}/script".display(),
    fp"{root_path}/lsusb".display(),
  )?
  assert result.comparison.matched_devices == 1
  assert result.comparison.matched_tree_rows == 1
  assert result.comparison.matched_descriptor_fields == 4
  assert result.comparison.mismatches.len() == 0 and result.comparison.partial.len() == 0
}
