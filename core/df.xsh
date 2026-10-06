#!/bin/xsh
use lib.gnu
use lib.disk_usage as disk

type DfOptions = {
  all: Bool, kilo: Bool, mega: Bool, human: Bool, si: Bool, block: List[Str],
  portable: Bool, inodes: Bool, show_type: Bool, local: Bool, total: Bool,
  sync: Bool, no_sync: Bool, include: List[Str], exclude: List[Str],
  output: List[Str], help: Bool, version: Bool, targets: List[Str],
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

proc main(...argv: List[Str]) [fs, env, process, io, error] {
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
  var units = disk.argument_units(argv, disk.environment_units("df", opts.portable), ["B", "block-size", "t", "type", "x", "exclude-type"], "H")
  if opts.portable and units.human == 0 { units = {...units, label: f"{units.block}-blocks"} }
  var columns: List[Str] = []
  if ! opts.output.is_empty() {
    for selection in opts.output { columns = columns.extend(selection.split(",")) }
  } else if opts.inodes {
    columns = ["source", "itotal", "iused", "iavail", "ipcent", "target"]
  } else { columns = ["source", "size", "used", "avail", "pcent", "target"] }
  if opts.show_type { columns = ["source", "fstype"].extend(columns[1..]) }
  let headings: Map[Str] = {source: "Filesystem", fstype: "Type", size: units.label, used: "Used", avail: if units.human != 0 { "Avail" } else { "Available" }, pcent: if opts.portable { "Capacity" } else { "Use%" }, target: "Mounted on", file: "File", itotal: "Inodes", iused: "IUsed", iavail: "IFree", ipcent: "IUse%"}
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
    for name in opts.targets {
      match fp"{name}".resolve() {
        Ok(resolved) => {
          match fs.mount_for(resolved) {
            Ok(found) => mounts += [{source: found.filesystem, target: found.mounted_on, kind: found.fstype, dev: found.device, file: name}]
            Err(failure) => { gnu.name_error(name, failure)
              failed = true }
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
    let prefix = if left_column(columns[0]) { "" } else { " " }
    gnu.write_text(prefix + cells.join(" ") + "\n")
  }
  if failed { exit 1 }
}
