use core.lib.hardware
use core.lib.sys_usb as usb

test test_lsusb_typed_fixture_identifiers_and_filters {
  let root = fs.tempdir()?
  defer root.close()
  root.mkdir(p"sys/bus/usb/devices/1-2", parents: true)
  for item in [
    {name: "idVendor", value: "1234"}, {name: "idProduct", value: "5678"},
    {name: "busnum", value: "1"}, {name: "devnum", value: "3"},
    {name: "manufacturer", value: "Acme"}, {name: "product", value: "Widget"},
  ] { root.write(fp"sys/bus/usb/devices/1-2/{item.name}", f"{item.value}\n") }
  let inventory = usb.collect(root)
  assert inventory.devices.len() == 1
  let labels = hardware.parse_ids("1234  Acme\n\t5678  Widget\n")?
  let lines = hardware.usb_lines(inventory.devices, labels, {verbose: false, tree: false})?
  assert lines == ["Bus 001 Device 003: ID 1234:5678 Acme Widget"]
  assert hardware.usb_matches(inventory.devices[0], "1234:5678", "1:3")?
  assert ! hardware.usb_matches(inventory.devices[0], "", "1:4")?
  let hub = {...inventory.devices[0], sysfs_name: "usb1", is_root_hub: true, device_number: 1, parent_device_index: null}
  let child = {...inventory.devices[0], parent_device_index: 0, port_path: "2"}
  let tree = hardware.usb_lines([hub, child], labels, {verbose: false, tree: true})?
  assert tree[0].starts_with("/:  Bus 001")
  assert tree[1].starts_with("    |__ Port 2: Dev 3")
  assert hardware.usb_lines([hub, {...child, parent_device_index: 1}], labels, {verbose: false, tree: true}) is Err(_)
}

test test_lsusb_help_and_invalid_node_are_deterministic { |ctx|
  let root = test.temp_dir(ctx)?
  let script = fp"{ctx.core_dir}/lsusb.xsh"
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let help = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "--help"], root, {}, b"", out, err, timeout: 5s)
  assert process.run(help)?.exited_with(0)
  assert "Usage: lsusb" in out.read_text()?
  let file = fp"{root}/descriptor"
  file.write(b"descriptor")
  let command = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "-D", file.display()], root, {}, b"", out, err, timeout: 5s)
  assert process.run(command)?.exited_with(1)
  assert "descriptor dump files are not supported" in err.read_text()?
}
