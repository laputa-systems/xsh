#!/bin/xsh
use lib.gnu
use lib.disk_usage as disk

type DfOptions = {
  all: Bool, kilo: Bool, mega: Bool, human: Bool, si: Bool, block: List[Str],
  portable: Bool, inodes: Bool, show_type: Bool, local: Bool, total: Bool,
  sync: Bool, no_sync: Bool, include: List[Str], exclude: List[Str],
  output: List[Str], verbose: Bool, help: Bool, version: Bool, targets: List[Str],
}
type Mount = {source: Str, target: Path, kind: Str, dev: Int, file: Str}
type Counters = {size: Int, used: Int, avail: Int, files: Int, iused: Int, ifree: Int}

# Compare fractional boundaries without multiplying filesystem counters by
# 100; counters can fit Int even when that intermediate product cannot.
pure percent(used: Int, available: Int) -> Str {
  let denominator = used + available
  return "-" when denominator <= 0 or used < 0
  let remainder = used % denominator
  var fraction = 0
  while fraction < 100 and remainder > denominator / 100 * fraction + denominator % 100 * fraction / 100 { fraction += 1 }
  f"{used / denominator * 100 + fraction}%"
}

pure row(mount: Mount, counts: Counters, units: disk.Units) -> Map[Str] {
  {
    source: mount.source, fstype: mount.kind, target: mount.target.display(), file: mount.file,
    size: disk.amount(counts.size, units), used: disk.amount(counts.used, units), avail: disk.amount(counts.avail, units),
    pcent: percent(counts.used, counts.avail), itotal: f"{counts.files}", iused: f"{counts.iused}",
    iavail: f"{counts.ifree}", ipcent: percent(counts.iused, counts.ifree),
  }
}

pure left_column(field: Str) -> Bool { field in ["source", "fstype", "target", "file"] }

# The option parser yields values without source offsets; retain a location in the original argv.
type ValueLocation = {index: Int, offset: Int}

pure diagnostic_spaces(count: Int) -> Str { [" " for _ in range(count)].join("") }

pure argument_source(program: Str, argv: List[Str]) -> Str {
  if argv.is_empty() { program } else { f"{program} {argv.join(" ")}" }
}

pure argument_column(program: Str, argv: List[Str], location: ValueLocation) -> Int {
  var prefix = program
  for index in range(location.index) { prefix = f"{prefix} {argv[index]}" }
  prefix.byte_len() + 2 + location.offset
}

proc report_size_error(program: Str, argv: List[Str], location: ValueLocation, value: Str, message: Str) [env, process, error] {
  gnu.error(message)
  return when ! unix.isatty(2)

  let source = argument_source(program, argv)
  let unsupported = message.starts_with("invalid suffix in")
  let offset = if unsupported { (rx"^(0[xX][0-9a-fA-F]+|0[bB][01]+|[0-9]+)".captures(value).get(0) ?? "").byte_len() } else { 0 }
  let column = argument_column(program, argv, location) + offset
  let width = value.byte_len() - offset
  let marker = ["─" for _ in range(if unsupported { width - 1 } else { width })].join("")
  let annotation = if unsupported { f"\n   │{diagnostic_spaces(column + 1)}╰── not a known unit" } else { "" }
  let tail = if unsupported { "\n   │\n   │ Help: a size is a number and an optional unit: K, M, G and so on for 1024, KB, MB, GB for 1000" } else { "\n   │" }
  eprint f"   ╭─[ {program}:1:{column} ]\n   │\n 1 │ {source}\n   │{diagnostic_spaces(column)}{marker}{if unsupported { "┬" } else { "" }}{annotation}{tail}\n───╯"
}

# Check explicit sizes before the shared unit selector emits an error without argv location.
proc validate_block_sizes(argv: List[Str]) [env, process, error] {
  var index = 0
  let posix = env.get("POSIXLY_CORRECT") is Ok(_)
  while index < argv.len() {
    let raw = argv[index]
    if raw == "--" or (posix and (raw == "-" or ! raw.starts_with("-"))) { break }
    var location: ValueLocation? = null
    var value = ""
    if raw.starts_with("--") {
      let equal = raw.find("=")
      let equal_at = equal ?? 0
      let name = if equal == null { raw.byte_slice(2) } else { raw.byte_slice(2, length: equal_at - 2) }
      if name != "" and "block-size".starts_with(name) {
        if let at = equal {
          location = {index: index, offset: at + 1}
          value = raw.byte_slice(at + 1)
        } else if index + 1 < argv.len() {
          index += 1
          location = {index: index, offset: 0}
          value = argv[index]
        }
      }
    } else if raw == "-B" {
      if index + 1 < argv.len() {
        index += 1
        location = {index: index, offset: 0}
        value = argv[index]
      }
    } else if raw.starts_with("-B") {
      location = {index: index, offset: 2}
      value = raw.byte_slice(2)
    }
    if let found = location {
      if value not in ["human-readable", "si"] {
        if let size = disk.parse_size(value) {
          if size <= 0 { report_size_error("df", argv, found, value, disk.size_error(value, "block-size")); exit 1 }
        } else {
          report_size_error("df", argv, found, value, disk.size_error(value, "block-size"))
          exit 1
        }
      }
    }
    index += 1
  }
}

proc main(...argv: List[Str]) [fs, env, process, io, error] {
  # GNU df accepts -v and ignores it; it is parsed so scripts that pass it still run and produce identical output.
  let opts: DfOptions = cli.applet(argv, {
    gnu: {status: 1},
    all: {form: "-a --all", default: false},
    kilo: {form: "-k", default: false}, mega: {form: "-m", default: false},
    human: {form: "-h --human-readable", default: false}, si: {form: "-H --si", default: false},
    block: {form: "-B --block-size SIZE", repeated: true},
    portable: {form: "-P --portability", default: false},
    inodes: {form: "-i --inodes", default: false},
    show_type: {form: "-T --print-type", default: false},
    local: {form: "-l --local", default: false}, total: {form: "--total", default: false},
    sync: {form: "--sync", default: false, conflicts: ["no_sync"]},
    no_sync: {form: "--no-sync", default: false, conflicts: ["sync"]},
    include: {form: "-t --type TYPE", repeated: true}, exclude: {form: "-x --exclude-type TYPE", repeated: true},
    output: {form: "--output[=FIELD_LIST]", repeated: true, optional_default: "source,fstype,itotal,iused,iavail,ipcent,size,used,avail,pcent,file,target"},
    help: {form: "--help", default: false, stop: true}, version: {form: "--version", default: false, stop: true},
    verbose: {form: "-v", default: false},
    targets: {form: "...FILE"},
  })?
  if opts.help {
    gnu.help("Usage: df [OPTION]... [FILE]...\nShow filesystem space.\n  -k  use 1024-byte blocks\n  -P, --portability  use the POSIX output format\n  -h, --human-readable  use powers of 1024\n  -H, --si  use powers of 1000\n  -i, --inodes  show inode usage\n  -B, --block-size=SIZE  select display units\n      --output[=FIELDS]  select columns\n  -a, --all  include duplicate and zero-size filesystems\n  -T, --print-type  show filesystem types\n  -t, --type=TYPE  select filesystem types\n  -x, --exclude-type=TYPE  exclude filesystem types\n  -l, --local  omit network filesystems\n      --total  show an aggregate row\n      --sync  synchronize before reading counters\n")
    return
  }
  if opts.version { gnu.version("df")
    return }
  if ! opts.output.is_empty() and (opts.portable or opts.inodes or opts.show_type) {
    gnu.usage_error("options --output and -i, -P or -T are mutually exclusive")
  }
  var invalid_types = false
  for kind in opts.include {
    if kind in opts.exclude {
      gnu.error(f"file system type {gnu.quote_value(kind)} both selected and excluded")
      invalid_types = true
    }
  }
  if invalid_types { exit 1 }
  validate_block_sizes(argv)
  var units = disk.argument_units(argv, disk.environment_units("df", opts.portable), ["B", "block-size", "t", "type", "x", "exclude-type"], "H")
  if opts.portable and units.human == 0 { units = {...units, label: f"{units.block}-blocks"} }
  var columns: List[Str] = []
  if ! opts.output.is_empty() {
    for selection in opts.output { columns = columns.extend(selection.split(",")) }
  } else if opts.inodes {
    columns = ["source", "itotal", "iused", "iavail", "ipcent", "target"]
  } else { columns = ["source", "size", "used", "avail", "pcent", "target"] }
  if opts.show_type { columns = ["source", "fstype"].extend(columns[1..]) }
  # GNU labels the avail column "Avail" under --output and -h, and "Available" otherwise.
  let headings: Map[Str] = {source: "Filesystem", fstype: "Type", size: units.label, used: "Used", avail: if units.human != 0 or ! opts.output.is_empty() { "Avail" } else { "Available" }, pcent: if opts.portable { "Capacity" } else { "Use%" }, target: "Mounted on", file: "File", itotal: "Inodes", iused: "IUsed", iavail: "IFree", ipcent: "IUse%"}
  var unique: List[Str] = []
  for column in columns {
    if column not in headings { gnu.usage_error(f"field {gnu.quote_value(column)} unknown") }
    if column in unique { gnu.usage_error(f"option --output: field {gnu.quote_value(column)} used more than once") }
    unique += [column]
  }
  if opts.sync { fs.sync() }
  var mounts: List[Mount] = []
  var failed = false
  if opts.targets.is_empty() {
    for found in fs.mounts()? {
      mounts += [{source: found.filesystem, target: found.mounted_on, kind: found.fstype, dev: found.device, file: "-"}]
    }
  } else {
    # Filters and -a/-l classify mounts from the table, so they cannot run without it;
    # a plain operand is still measured from its own statvfs counters.
    let fallback_allowed = ! opts.all and ! opts.local and opts.include.is_empty() and opts.exclude.is_empty()
    for name in opts.targets {
      match fp"{name}".resolve() {
        Ok(resolved) => {
          match fs.mount_for(resolved) {
            Ok(found) => mounts += [{source: found.filesystem, target: found.mounted_on, kind: found.fstype, dev: found.device, file: name}]
            Err(failure) => {
              match fs.mounts() {
                Ok(_) => { gnu.name_error(name, failure)
                  failed = true }
                Err(table_failure) => {
                  gnu.error(f"cannot read table of mounted file systems: {gnu.strerror(table_failure)}")
                  if ! fallback_allowed { exit 1 }
                  # dev is only compared when listing the whole table, so an operand has no device identity.
                  mounts += [{source: "-", target: fp"{name}", kind: "-", dev: 0, file: name}]
                }
              }
            }
          }
        }
        Err(failure) => { gnu.name_error(name, failure)
          failed = true }
      }
    }
  }
  var table: List[Map[Str]] = []
  var devices: List[Int] = []
  var totals: Counters = {size: 0, used: 0, avail: 0, files: 0, iused: 0, ifree: 0}
  for mount in mounts {
    continue when ! opts.include.is_empty() and mount.kind not in opts.include
    continue when mount.kind in opts.exclude
    continue when opts.local and mount.kind in ["nfs", "nfs4", "cifs", "smbfs", "afs", "ceph", "9p", "fuse.sshfs", "fuse.rclone"]
    if opts.targets.is_empty() and ! opts.all {
      continue when mount.kind in ["proc", "sysfs", "devpts", "cgroup", "cgroup2", "mqueue", "securityfs", "pstore", "debugfs", "tracefs", "autofs"]
      continue when mount.dev in devices
    }
    guard let stats = fs.statvfs(mount.target) else { |failure|
      gnu.name_error(mount.target.display(), failure)
      failed = true
      continue
    }
    continue when opts.targets.is_empty() and ! opts.all and stats.blocks == 0
    devices += [mount.dev]
    let counts: Counters = {size: stats.blocks * stats.fragment_size, used: (stats.blocks - stats.blocks_free) * stats.fragment_size, avail: stats.blocks_available * stats.fragment_size, files: stats.files, iused: stats.files - stats.files_free, ifree: stats.files_available}
    totals = {size: totals.size + counts.size, used: totals.used + counts.used, avail: totals.avail + counts.avail, files: totals.files + counts.files, iused: totals.iused + counts.iused, ifree: totals.ifree + counts.ifree}
    table += [row(mount, counts, units)]
  }
  if table.is_empty() {
    if ! failed { gnu.error("no file systems processed") }
    exit 1
  }
  if opts.total {
    var total_row = row({source: "total", target: p"-", kind: "-", dev: 0, file: "-"}, totals, units)
    if "source" not in columns and "target" in columns { total_row["target"] = "total" }
    table += [total_row]
  }
  var widths: List[Int] = []
  for column in columns {
    var width = headings[column].count_chars()
    for data in table { if data[column].count_chars() > width { width = data[column].count_chars() } }
    widths += [width]
  }
  for data in [headings].extend(table) {
    var cells: List[Str] = []
    for index in range(columns.len()) {
      let field = columns[index]
      let text = data[field]
      let padding = [" " for _ in range(widths[index] - text.count_chars())].join("")
      cells += [if left_column(field) { text + (if index == columns.len() - 1 { "" } else { padding }) } else { padding + text }]
    }
    gnu.write_text(cells.join(" ") + "\n")
  }
  if failed { exit 1 }
}
