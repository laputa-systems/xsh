use core.lib.hardware
use core.lib.sys_pci as pci

pure device_fixture() -> pci.PciFunction {
  {
    address: "0001:02:03.4", domain: 1, bus: 2, device: 3, function: 4,
    vendor_id: 4660, device_id: 22136, subsystem_vendor_id: null,
    subsystem_device_id: null, class_code: 131072, revision: 2,
    driver: "acme", parent_function_index: null, numa_node: 0, iommu_group: null,
    current_link_speed: null, current_link_width: null, maximum_link_speed: null,
    maximum_link_width: null,
  }
}

test test_lspci_labels_numeric_modes_and_selectors {
  let labels = hardware.parse_ids("1234  Acme\n\t5678  Network device\nC 02  Network controller\n\t00  Ethernet controller\n")?
  let spaced = hardware.parse_ids("1234  Acme  Corp\n\t5678  Widget  V2\n")?
  assert spaced.get("1234")? == "Acme  Corp"
  assert spaced.get("1234:5678")? == "Widget  V2"
  let device = device_fixture()
  let view: hardware.PciView = {numeric: 2, driver: true, verbose: 0, domain: true, tree: false}
  let lines = hardware.pci_lines([device], labels, view)?
  assert lines[0] == "0001:02:03.4 Ethernet controller [0200]: Acme Network device [1234:5678] (rev 02)"
  assert lines[1] == "\tKernel driver in use: acme"
  assert hardware.pci_matches(device, "02:03.4", "1234:5678")?
  assert hardware.pci_matches(device, "", "1234:5678:0200:00")?
  assert ! hardware.pci_matches(device, "0000:02:03.4", "")?
  assert ! hardware.pci_matches(device, "", "1234:9999")?
  assert hardware.pci_matches(device, "*.4", "*:5678")?
}

test test_lspci_invalid_selectors_fail {
  for selector in ["00:99.0", "00:01.8", "garbage", "0000:00:01:02.0"] {
    assert hardware.pci_selector(selector) is Err(_)
  }
}

test test_lspci_help_and_unsupported_register_reads_are_deterministic { |ctx|
  let root = test.temp_dir(ctx)?
  let script = fp"{ctx.core_dir}/lspci.xsh"
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let help = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "--help"], root, {}, b"", out, err, timeout: 5s)
  assert process.run(help)?.exited_with(0)
  assert "Usage: lspci" in out.read_text()?
  let registers = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "-xxx"], root, {}, b"", out, err, timeout: 5s)
  assert process.run(registers)?.exited_with(1)
  assert "not supported" in err.read_text()?
}


test test_lspci_tree_retains_parent_indexes_and_rejects_unreachable_cycles {
  let bridge = {...device_fixture(), address: "0000:00:01.0", domain: 0, bus: 0, device: 1, function: 0, class_code: 394240, parent_function_index: null}
  let child = {...device_fixture(), address: "0000:02:00.0", domain: 0, bus: 2, device: 0, function: 0, parent_function_index: 0}
  let labels: Map[Str] = {}
  let view: hardware.PciView = {numeric: 1, driver: false, verbose: 0, domain: true, tree: true}
  let lines = hardware.pci_lines([bridge, child], labels, view)?
  assert lines.len() == 2
  assert lines[1].starts_with("  +-0000:02:00.0")
  let broken = {...child, parent_function_index: 1}
  assert hardware.pci_lines([bridge, broken], labels, view) is Err(_)
}
