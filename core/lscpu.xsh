#!/bin/xsh
use lib.gnu
use lib.hardware
use lib.system_report as report

type Options = {extended: Str?, parse: Str?, all: Bool, online: Bool, offline: Bool, physical: Bool, bytes_mode: Bool, json: Bool, sysroot: Path?, help: Bool, version: Bool, operands: List[Str]}

proc inventory(opts: Options) -> Result[Unit] {
  let architecture = system.uname()?.machine
  let section = if opts.sysroot != null {
    let root = fs.open_root(opts.sysroot)?
    defer root.close()
    hardware.cpu_from_root(root, architecture)?
  } else { hardware.cpu_live()? }
  if ! section.status.enumeration_succeeded { gnu.error("cannot enumerate CPUs"); exit 1 }
  if opts.extended != null and opts.parse != null { gnu.usage_error("options --extended and --parse are mutually exclusive") }
  if opts.all and (opts.online or opts.offline) { gnu.usage_error("options --all, --online and --offline are mutually exclusive") }
  if opts.online and opts.offline { gnu.usage_error("options --online and --offline are mutually exclusive") }
  if opts.extended != null or opts.parse != null {
    if opts.bytes_mode { gnu.usage_error("--bytes requires cache size summaries") }
    if opts.json { gnu.usage_error("JSON CPU rows are not available; use summary --json") }
    let selection = if opts.all { "all" } else if opts.offline { "offline" } else if opts.online or opts.parse != null { "online" } else { "all" }
    let columns = (opts.extended ?? opts.parse ?? "CPU,NODE,SOCKET,CORE,ONLINE").split(",")
    for line in hardware.cpu_rows(section, columns, selection, opts.parse != null, opts.physical)? { gnu.write_text(line + "\n") }
  } else {
    if opts.all or opts.online or opts.offline or opts.physical { gnu.usage_error("CPU selection requires --extended or --parse") }
    let rows = hardware.cpu_summary(section, architecture, opts.bytes_mode)
    if opts.json { gnu.write_text(json.encode({lscpu: rows}, pretty: true)? + "\n") } else { for row in rows { print f"{row.field:<28} {row.data}" } }
  }
}

proc main(...argv: List[Str]) {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 1, unsupported: {"-x": "hexadecimal CPU masks are not available", "--hex": "hexadecimal CPU masks are not available", "-C": "cache-table output is not available", "--caches": "cache-table output is not available", "--output-all": "all CPU columns are not available", "--hierarchic": "hierarchical summary output is not available"}},
    extended: {form: "-e --extended[=LIST]", optional_default: "CPU,NODE,SOCKET,CORE,ONLINE"},
    parse: {form: "-p --parse[=LIST]", optional_default: "CPU,CORE,SOCKET,NODE"},
    all: {form: "-a --all", default: false}, online: {form: "-b --online", default: false}, offline: {form: "-c --offline", default: false},
    bytes_mode: {form: "-B --bytes", default: false},
    physical: {form: "-y --physical", default: false},
    json: {form: "-J --json", default: false}, sysroot: {form: "-s --sysroot DIR", kind: "Path"},
    help: {form: "-h --help", default: false, stop: true}, version: {form: "-V --version", default: false, stop: true}, operands: {form: "...ARG"},
  })?
  if opts.help { gnu.help("Usage: lscpu [-J] [-B] [-e[=LIST] | -p[=LIST]] [-a|-b|-c] [-y] [-s DIR]\nDisplay CPU topology, identity and cache information."); return }
  if opts.version { gnu.version("lscpu"); return }
  if ! opts.operands.is_empty() { gnu.extra_operand(opts.operands[0]) }
  if let Err(failure) = inventory(opts) { gnu.error(failure.message); exit 1 }
}
