#!/bin/xsh
use lib.gnu
use lib.hardware
use lib.sys_usb as usb

type Options = {verbose: Bool, tree: Bool, ids: Str, slot: Str, device: Path?, help: Bool, version: Bool, operands: List[Str]}

proc inventory(opts: Options) -> Result[Unit] {
  var slot = opts.slot
  if opts.device != null {
    let device_path = opts.device
    let metadata = fs.stat(device_path, follow_symlinks: true)?
    if metadata.kind != "char" { gnu.usage_error("-D requires a USB device node; descriptor dump files are not supported") }
    let parts = opts.device.display().split("/")
    if parts.len() < 3 or ! opts.device.display().starts_with("/dev/bus/usb/") { gnu.usage_error("-D requires a /dev/bus/usb/BUS/DEVICE node") }
    let bus = parts[-2].parse_int()?
    let number = parts[-1].parse_int()?
    slot = f"{bus}:{number}"
  }
  let _ = hardware.usb_selector(slot)?
  if opts.ids.split(":").len() > 2 { gnu.usage_error("USB identifiers accept vendor:product only") }
  let selector = hardware.id_selector(opts.ids)?
  if selector.class != null or selector.program != null { gnu.usage_error("USB identifiers accept vendor:product only") }
  if opts.tree and (opts.ids != "" or slot != "") { gnu.usage_error("USB tree output cannot be combined with device selectors") }
  let names = hardware.load_labels([p"/usr/share/hwdata/usb.ids", p"/usr/share/misc/usb.ids", p"/var/lib/usbutils/usb.ids"])?
  let root = fs.open_root(/)?
  defer root.close()
  let collected = usb.collect(root)
  if ! collected.enumeration_succeeded { gnu.error("cannot enumerate USB devices"); exit 1 }
  for issue in collected.issues { gnu.error(f"cannot read {issue.field}: {issue.error_kind ?? "source unavailable"}") }
  let devices = if opts.tree { collected.devices } else { collected.devices |> where { |device| hardware.usb_matches(device, opts.ids, slot)? } }
  if devices.is_empty() and (opts.ids != "" or slot != "") { exit 1 }
  for line in hardware.usb_lines(devices, names, {verbose: opts.verbose or opts.device != null, tree: opts.tree})? { gnu.write_text(line + "\n") }
}

proc main(...argv: List[Str]) {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 1, unsupported: {"-z": "usbutils extension output is not available"}},
    verbose: {form: "-v --verbose", default: false}, tree: {form: "-t --tree", default: false},
    ids: {form: "-d ID", default: ""}, slot: {form: "-s SLOT", default: ""}, device: {form: "-D DEVICE", kind: "Path"},
    help: {form: "-h --help", default: false, stop: true}, version: {form: "-V --version", default: false, stop: true}, operands: {form: "...ARG"},
  })?
  if opts.help { gnu.help("Usage: lsusb [-v] [-t] [-d VENDOR:PRODUCT] [-s BUS:DEVICE] [-D DEVICE]\nList USB devices and descriptors."); return }
  if opts.version { gnu.version("lsusb"); return }
  if ! opts.operands.is_empty() { gnu.extra_operand(opts.operands[0]) }
  if let Err(failure) = inventory(opts) { gnu.error(failure.message); exit 1 }
}
