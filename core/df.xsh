#!/bin/xsh
use lib.gnu

type DfOptions = {
  all: Bool,
  block_size: Str?,
  human: Bool,
  si: Bool,
  inodes: Bool,
  kilobytes: Bool,
  megabytes: Bool,
  local: Bool,
  portability: Bool,
  total: Bool,
  print_type: Bool,
  output: Str,
  sync: Bool,
  no_sync: Bool,
  types: List[Str],
  exclude_types: List[Str],
  targets: List[Str],
  help: Bool,
  version: Bool,
}

type SizeParse = {value: Int, issue: Str}

pure parse_digits(text: Str, radix: Int) -> Int? {
  return null when text == ""
  var value = 0
  for index in range(text.byte_len()) {
    let digit = "0123456789abcdef".find(text.byte_slice(index, length: 1)) ?? -1
    return null when digit < 0 or digit >= radix
    return -1 when value > 9007199254740991 / radix
    value = value * radix + digit
  }
  value
}

pure parse_size(text: Str) -> SizeParse {
  var number = text
  var selected = ""
  var factor = 1
  for candidate in ["YiB", "ZiB", "YB", "ZB", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB", "KB", "MB", "GB", "TB", "PB", "EB", "kB", "K", "M", "G", "T", "P", "E", "Y", "Z", "B"] {
    if text.ends_with(candidate) {
      selected = candidate
      number = text.byte_slice(0, length: text.byte_len() - candidate.byte_len())
      if candidate in ["YiB", "ZiB", "YB", "ZB", "Y", "Z"] {
        return {value: 0, issue: "too large"}
      }
      factor = if candidate in ["KB", "kB"] { 1000 } else if candidate == "MB" { 1000000 } else if candidate == "GB" { 1000000000 } else if candidate == "TB" { 1000000000000 } else if candidate == "PB" { 1000000000000000 } else if candidate == "EB" { 1000000000000000 } else if candidate in ["K", "KiB"] { 1024 } else if candidate in ["M", "MiB"] { 1048576 } else if candidate in ["G", "GiB"] { 1073741824 } else if candidate in ["T", "TiB"] { 1099511627776 } else if candidate in ["P", "PiB"] { 1125899906842624 } else if candidate == "EiB" { 1125899906842624 } else { 1 }
      break
    }
  }
  let binary = number.starts_with("0b")
  let digits = if binary { number.byte_slice(2) } else { number }
  let value: Int? = if digits == "" and selected != "" { 1 } else { parse_digits(digits, if binary { 2 } else { 10 }) }
  if value == null {
    let starts_numeric = number.byte_slice(0, length: 1) in ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"]
    let issue = if number == "" or number == "0b" { "invalid" } else if binary and digits != "" { "suffix" } else if number.byte_slice(0, length: 1) in ["+", "-"] or ! starts_numeric { "invalid" } else { "suffix" }
    return {value: 0, issue: issue}
  }
  let parsed = value ?? 0
  if parsed < 0 { return {value: 0, issue: "too large"} }
  if parsed == 0 { return {value: 0, issue: if selected == "B" { "suffix" } else { "invalid"} } }
  if parsed < 0 { return {value: 0, issue: "invalid"} }
  if parsed > 9007199254740 or parsed * factor > 9007199254740991 {
    return {value: 0, issue: "too large"}
  }
  {value: parsed * factor, issue: ""}
}

pure ceil_div(value: Int, unit: Int) -> Int {
  return 0 when value == 0
  (value + unit - 1) / unit
}

pure block_label(unit: Int) -> Str {
  if unit % 1073741824 == 0 { return f"{unit / 1073741824}G" }
  if unit % 1048576 == 0 { return f"{unit / 1048576}M" }
  if unit % 1000000000000 == 0 { return f"{unit / 1000000000000}TB" }
  if unit % 1000000000 == 0 { return f"{unit / 1000000000}GB" }
  if unit % 1000000 == 0 { return f"{unit / 1000000}MB" }
  if unit % 1000 == 0 and unit / 1000 < 1000 { return f"{unit / 1000}kB" }
  if unit % 1024 == 0 and unit / 1024 < 1000 { return f"{unit / 1024}K" }
  if unit % 1024 == 0 and unit / 1024 >= 1000 {
    let tenths = ceil_div(unit * 10, 1000000)
    return f"{tenths / 10}.{tenths % 10}MB" when tenths % 10 != 0
    return f"{tenths / 10}MB"
  }
  f"{unit}B"
}

pure block_header(unit: Int, portability: Bool, human: Bool, si: Bool) -> Str {
  return "Size" when human or si
  return f"{unit}-blocks" when portability
  f"{block_label(unit)}-blocks"
}

pure human_size(bytes_count: Int, decimal: Bool) -> Str {
  let base = if decimal { 1000 } else { 1024 }
  let suffixes = if decimal { ["k", "M", "G", "T", "P", "E"] } else { ["K", "M", "G", "T", "P", "E"] }
  return f"{bytes_count}B" when bytes_count < base
  var value = bytes_count
  var index = 0
  while value >= base and index < suffixes.len() {
    value = value / base
    index += 1
  }
  let suffix = suffixes[index - 1]
  var denominator = 1
  var scale_count = 0
  while scale_count < index {
    denominator *= base
    scale_count += 1
  }
  let tenths = bytes_count % denominator * 10 / denominator
  let shown = if value >= 10 { f"{value}" } else { f"{value}.{tenths}" }
  f"{shown}{suffix}"
}

pure field_label(field: Str, size_label: Str) -> Str {
  match field {
    "source" => "Filesystem"
    "fstype" => "Type"
    "itotal" => "Inodes"
    "iused" => "IUsed"
    "iavail" => "IFree"
    "ipcent" => "IUse%"
    "size" => size_label
    "used" => "Used"
    "avail" => "Avail"
    "pcent" => "Use%"
    "file" => "File"
    "target" => "Mounted on"
    else => field
  }
}

pure field_value(field: Str, mount: FsMount, target: Str, has_target: Bool, unit: Int, opts: DfOptions) -> Str {
  match field {
    "source" => mount.filesystem
    "fstype" => mount.fstype
    "itotal" => f"{mount.files}"
    "iused" => f"{mount.files_used}"
    "iavail" => f"{mount.files_free}"
    "ipcent" => if mount.files == 0 { "-" } else { f"{mount.files_capacity_percent}%" }
    "size" => if opts.human or opts.si { human_size(mount.blocks_1k * 1024, opts.si) } else { f"{ceil_div(mount.blocks_1k * 1024, unit)}" }
    "used" => if opts.human or opts.si { human_size(mount.used_1k * 1024, opts.si) } else { f"{ceil_div(mount.used_1k * 1024, unit)}" }
    "avail" => if opts.human or opts.si { human_size(mount.available_1k * 1024, opts.si) } else { f"{ceil_div(mount.available_1k * 1024, unit)}" }
    "pcent" => f"{mount.capacity_percent}%"
    "file" => if has_target { target } else { "-" }
    "target" => mount.mounted_on.display()
    else => ""
  }
}

pure is_known_field(field: Str) -> Bool {
  field in ["source", "fstype", "itotal", "iused", "iavail", "ipcent", "size", "used", "avail", "pcent", "file", "target"]
}

pure selected_mount(mount: FsMount, opts: DfOptions) -> Bool {
  return false when ! opts.all and mount.fstype == "binfmt_misc"
  return false when opts.local and mount.fstype in ["nfs", "nfs4", "cifs", "smb3", "sshfs", "fuse.sshfs"]
  if opts.types.len() > 0 and ! (mount.fstype in opts.types) { return false }
  if mount.fstype in opts.exclude_types { return false }
  true
}

proc main(...argv: List[Str]) [fs, error, io, env] {
  let opts: DfOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      all: {form: "-a --all", default: false},
      block_size: {form: "-B --block-size SIZE"},
      human: {form: "-h --human-readable", default: false, conflicts: ["si"]},
      si: {form: "-H --si", default: false, conflicts: ["human"]},
      inodes: {form: "-i --inodes", default: false},
      kilobytes: {form: "-k", default: false},
      megabytes: {form: "-m", default: false},
      local: {form: "-l --local", default: false},
      portability: {form: "-P --portability", default: false},
      total: {form: "--total", default: false},
      print_type: {form: "-T --print-type", default: false},
      output: {form: "--output[=FIELDS]", default: "", optional_default: "source,size,used,avail,pcent,target"},
      sync: {form: "--sync", default: false, conflicts: ["no_sync"]},
      no_sync: {form: "--no-sync", default: false, conflicts: ["sync"]},
      types: {form: "-t --type TYPE", repeated: true},
      exclude_types: {form: "-x --exclude-type TYPE", repeated: true},
      targets: {form: "...FILE"},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
    },
  )?

  if opts.help {
    gnu.help("""Usage: df [OPTION]... [FILE]...
Show information about the file system on which each FILE resides.

  -a, --all             include pseudo, duplicate, inaccessible file systems
  -B, --block-size=SIZE scale sizes by SIZE before printing them
  -h, --human-readable  print sizes in powers of 1024
  -H, --si              print sizes in powers of 1000
  -i, --inodes          list inode information instead of block usage
  -k                    use 1024-byte blocks
  -m                    use 1 MiB blocks
  -l, --local           limit listing to local file systems
  -P, --portability     use the POSIX output format
  -T, --print-type      print file system type
      --total           produce a grand total
      --output=FIELDS   use the output format defined by FIELDS
  -t, --type=TYPE       limit listing to file systems of type TYPE
  -x, --exclude-type=TYPE  exclude file systems of type TYPE
      --help            display this help and exit
      --version         output version information and exit""")
    return
  }

  if opts.version { gnu.version("df"); return }

  var output = opts.output
  var explicit_output: List[Str] = []
  for arg in argv {
    if arg.starts_with("--output=") {
      let list = arg.byte_slice(9).split(",")
      for field in list {
        if field in explicit_output {
          gnu.usage_error(f"option --output: field {gnu.quote(field)} used more than once")
        }
        explicit_output += [field]
      }
    }
  }
  if explicit_output.len() > 0 { output = explicit_output.join(",") }

  if output != "" and (opts.inodes or opts.portability or opts.print_type) {
    gnu.error("the --output option cannot be combined with -i, -P, or -T")
    exit 1
  }

  var conflicts: List[Str] = []
  for kind in opts.types {
    if kind in opts.exclude_types and ! (kind in conflicts) {
      conflicts += [kind]
      gnu.error(f"file system type {gnu.quote(kind)} both selected and excluded")
    }
  }
  if conflicts.len() > 0 { exit 1 }

  let posix = env.get("POSIXLY_CORRECT") is Ok(_)
  var unit = if posix { 512 } else { 1024 }
  if opts.kilobytes { unit = 1024 }
  if opts.megabytes { unit = 1048576 }

  if let requested = opts.block_size {
    let parsed = parse_size(requested)
    if parsed.issue != "" {
      if parsed.issue == "too large" {
        gnu.error(f"--block-size argument {gnu.quote(requested)} too large")
      } else if parsed.issue == "suffix" {
        gnu.error(f"invalid suffix in --block-size argument {gnu.quote(requested)}")
      } else {
        gnu.error(f"invalid --block-size argument {gnu.quote(requested)}")
      }
      exit 1
    }
    unit = parsed.value
  } else if ! posix and ! opts.portability and ! opts.kilobytes and ! opts.megabytes {
    for name in ["DF_BLOCK_SIZE", "BLOCK_SIZE", "BLOCKSIZE"] {
      if let Ok(raw) = env.get(name) {
        if raw != "" {
          let parsed = parse_size(raw)
          if parsed.issue == "" { unit = parsed.value }
          break
        }
      }
    }
  }

  if opts.sync { fs.sync()? }

  var fields = if output != "" { output.split(",") } else if opts.inodes { ["source", "itotal", "iused", "iavail", "ipcent", "target"] } else if opts.print_type { ["source", "fstype", "size", "used", "avail", "pcent", "target"] } else { ["source", "size", "used", "avail", "pcent", "target"] }
  if output != "" {
    for field in fields {
      if ! is_known_field(field) {
        gnu.error(f"invalid field {gnu.quote(field)}")
        exit 1
      }
    }
    for index in range(fields.len()) {
      for prior in range(index) {
        if fields[index] == fields[prior] {
          gnu.error(f"option --output: field {gnu.quote(fields[index])} used more than once")
          exit 1
        }
      }
    }
  }

  let size_label = block_header(unit, opts.portability, opts.human, opts.si)
  var mounts: List[FsMount] = []
  var target_names: List[Str] = []
  var has_targets: List[Bool] = []
  var missing_target = false

  if opts.targets.len() == 0 {
    let all_mounts = fs.mounts()?.collect() |> sort-by .mounted_on.display()
    for mount in all_mounts {
      if selected_mount(mount, opts) {
        mounts += [mount]
        target_names += ["-"]
        has_targets += [false]
      }
    }
  } else {
    let fallback_allowed = ! opts.all and ! opts.local and opts.types.len() == 0 and opts.exclude_types.len() == 0
    for item in opts.targets {
      let item_path = fp"{item}"
      if ! item_path.exists()? {
        gnu.error(f"{item}: No such file or directory")
        missing_target = true
        continue
      }
      let mount_result = fs.mount_for(item_path.resolve()?)
      let mount = if let Ok(found) = mount_result {
        found
      } else if fallback_allowed {
        gnu.error("cannot read table of mounted file systems")
        let usage = fs.filesystem_stats(item_path)?
        let inodes = fs.statvfs(item_path)?
        {
          filesystem: "-",
          mounted_on: item_path,
          fstype: "-",
          device: inodes.fsid,
          blocks_1k: usage.blocks_1k,
          used_1k: usage.used_1k,
          available_1k: usage.available_1k,
          capacity_percent: usage.capacity_percent,
          files: inodes.files,
          files_used: inodes.files - inodes.files_free,
          files_free: inodes.files_free,
          files_capacity_percent: if inodes.files == 0 { 0 } else { ceil_div((inodes.files - inodes.files_free) * 100, inodes.files) },
          readonly: inodes.readonly,
        }
      } else {
        gnu.error("cannot read table of mounted file systems")
        exit 1
      }
      if selected_mount(mount, opts) {
        mounts += [mount]
        target_names += [item]
        has_targets += [true]
      }
    }
  }

  if mounts.len() == 0 and missing_target { exit 1 }
  if mounts.len() == 0 {
    gnu.error("no file systems processed")
    exit 1
  }

  var headers: List[Str] = []
  if output == "" and opts.portability and ! opts.inodes {
    headers = ["Filesystem", size_label, "Used", "Available", "Capacity", "Mounted on"]
  } else if output == "" and ! opts.human and ! opts.si and ! opts.inodes {
    headers = ["Filesystem", size_label, "Used", "Available", "Use%", "Mounted on"]
  } else {
    for field in fields { headers += [field_label(field, size_label)] }
  }
  var header = if headers.len() == 1 and fields[0] in ["size", "used", "avail", "pcent", "itotal", "iused", "iavail", "ipcent"] and (opts.human or opts.si) { f" {headers[0]}" } else { headers.join(" ") }
  if fields.len() == 2 and fields[0] == "file" and fields[1] == "target" {
    var width = 4
    for name in target_names { width = if name.count_chars() > width { name.count_chars() } else { width } }
    var spaces = " "
    var at = 4
    while at < width { spaces = f"{spaces} "; at += 1 }
    header = f"File{spaces}Mounted on"
  }
  print $header

  var total_size = 0
  var total_used = 0
  var total_avail = 0
  var total_inodes = 0
  var total_iused = 0
  var total_iavail = 0
  for index in range(mounts.len()) {
    let mount = mounts[index]
    total_size += ceil_div(mount.blocks_1k * 1024, unit)
    total_used += ceil_div(mount.used_1k * 1024, unit)
    total_avail += ceil_div(mount.available_1k * 1024, unit)
    total_inodes += mount.files
    total_iused += mount.files_used
    total_iavail += mount.files_free
    var values: List[Str] = []
    for field in fields {
      values += [field_value(field, mount, target_names[index], has_targets[index], unit, opts)]
    }
    print values.join(" ")
  }

  if opts.total {
    var values: List[Str] = []
    for field in fields {
      values += [match field {
        "source" => "total"
        "target" => if "source" in fields { "-" } else { "total" }
        "size" => f"{total_size}"
        "used" => f"{total_used}"
        "avail" => f"{total_avail}"
        "itotal" => f"{total_inodes}"
        "iused" => f"{total_iused}"
        "iavail" => f"{total_iavail}"
        "pcent" => f"{ceil_div(total_used * 100, total_used + total_avail)}%"
        "ipcent" => if total_inodes == 0 { "-" } else { f"{ceil_div(total_iused * 100, total_inodes)}%" }
        else => "-"
      }]
    }
    print values.join(" ")
  }

  if missing_target { exit 1 }
}
