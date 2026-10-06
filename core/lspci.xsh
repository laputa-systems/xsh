#!/bin/xsh
use lib.gnu
use lib.hardware
use lib.sys_pci as pci

type Options = {numeric: Bool, driver: Bool, verbose: Bool, domain: Bool, tree: Bool, slot: Str, ids: Str, database: Path?, help: Bool, version: Bool, operands: List[Str]}

proc inventory(opts: Options, argv: List[Str]) -> Result[Unit] {
  let tokens = cli.tokens(argv, ["s", "d", "i"])?
  let numeric = tokens |> where .kind == "short" and .name == "n" |> count()
  let verbose = tokens |> where .kind == "short" and .name == "v" |> count()
  if verbose > 1 { gnu.usage_error("-vv requires PCI configuration and capability decoding, which is not available") }
  let _ = hardware.pci_selector(opts.slot)?
  let _ = hardware.id_selector(opts.ids)?
  let labels = if opts.database != null { hardware.parse_ids(opts.database.read_text()?)? } else { hardware.load_labels([p"/usr/share/hwdata/pci.ids", p"/usr/share/misc/pci.ids"])? }
  let root = fs.open_root(/)?
  defer root.close()
  let collected = pci.collect(root)
  if ! collected.status.enumeration_succeeded { gnu.error("cannot enumerate PCI devices"); exit 1 }
  for issue in collected.issues { gnu.error(f"cannot read {issue.field}: {issue.error_kind ?? "source unavailable"}") }
  # Filter after collection so tree relationships retain their inventory indexes.
  if opts.tree and (opts.slot != "" or opts.ids != "") { gnu.usage_error("PCI tree output cannot be combined with device selectors") }
  let devices = if opts.tree { collected.functions } else { collected.functions |> where { |device| hardware.pci_matches(device, opts.slot, opts.ids)? } }
  let view: hardware.PciView = {numeric: numeric, driver: opts.driver, verbose: verbose, domain: opts.domain, tree: opts.tree}
  for line in hardware.pci_lines(devices, labels, view)? { gnu.write_text(line + "\n") }
}

proc main(...argv: List[Str]) {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 1, unsupported: {"-x": "PCI register reads are not available", "-m": "machine-readable PCI output is not available", "-A": "alternate PCI access methods are not available", "-H": "direct hardware access is not available", "-O": "access-method parameters are not available", "-F": "PCI register dump replay is not available", "-b": "bus-centric PCI addresses are not available", "-q": "network database queries are not available", "-Q": "network database queries are not available", "-M": "PCI bus mapping is not available", "-P": "bridge path addresses are not available"}},
    numeric: {form: "-n", default: false}, driver: {form: "-k", default: false}, verbose: {form: "-v", default: false},
    domain: {form: "-D", default: false}, tree: {form: "-t", default: false},
    slot: {form: "-s SLOT", default: ""}, ids: {form: "-d ID", default: ""}, database: {form: "-i FILE", kind: "Path"},
    help: {form: "-h --help", default: false, stop: true}, version: {form: "-V --version", default: false, stop: true}, operands: {form: "...ARG"},
  })?
  if opts.help { gnu.help("Usage: lspci [-n|-nn] [-k] [-v] [-D] [-t] [-s SLOT] [-d ID] [-i FILE]\nList PCI devices."); return }
  if opts.version { gnu.version("lspci"); return }
  if ! opts.operands.is_empty() { gnu.extra_operand(opts.operands[0]) }
  if let Err(failure) = inventory(opts, argv) { gnu.error(failure.message); exit 1 }
}
