#!/bin/xsh
use lib.gnu
use lib.disk_usage as disk

type DuOptions = {
  summarize: Bool, all: Bool, total: Bool, apparent: Bool, bytes: Bool,
  human: Bool, si: Bool, kilo: Bool, mega: Bool, block: List[Str],
  depth: Str?, count_links: Bool, follow_all: Bool, follow_args: Bool, no_follow: Bool,
  separate: Bool, one_file_system: Bool, inodes: Bool, zero: Bool, threshold: Str?,
  excludes: List[Str], exclude_files: List[Str], files0: Str?,
  time: Str?, time_style: Str?, verbose: Bool, help: Bool, version: Bool, targets: List[Str],
}
type Policy = {
  all: Bool, total: Bool, apparent: Bool, count_links: Bool, follow_all: Bool,
  follow_args: Bool, separate: Bool, one_file_system: Bool, inodes: Bool,
  depth: Int, threshold: Int?, negative: Bool, units: disk.Units,
  excludes: List[Regex], ending: Str, time: Str, time_style: Str, verbose: Bool,
}
type Usage = {size: Int, accounted: Int, counted: Bool, links: Map[Bool], latest: Int, failed: Bool, directory: Bool}
type EntryMetadata = {meta: FsStat, external_target: Path?}
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

  let unsupported = message.starts_with("invalid suffix in")
  let signed_threshold = unsupported and value.starts_with("-") and message.find("--threshold") != null
  let prefix_value = if signed_threshold { value.byte_slice(1) } else { value }
  let prefix = if unsupported { (rx"^(0[xX][0-9a-fA-F]+|0[bB][01]+|[0-9]+)".captures(prefix_value).get(0) ?? "").byte_len() } else { 0 }
  let offset = prefix + (if signed_threshold { 1 } else { 0 })
  let column = argument_column(program, argv, location) + offset
  let source = argument_source(program, argv)
  let width = value.byte_len() - offset
  let marker = ["─" for _ in range(if unsupported { width - 1 } else { width })].join("")
  let annotation = if unsupported { f"\n   │{diagnostic_spaces(column + 1)}╰── not a known unit" } else { "" }
  let tail = if unsupported { "\n   │\n   │ Help: a size is a number and an optional unit: K, M, G and so on for 1024, KB, MB, GB for 1000" } else { "\n   │" }
  eprint f"   ╭─[ {program}:1:{column} ]\n   │\n 1 │ {source}\n   │{diagnostic_spaces(column)}{marker}{if unsupported { "┬" } else { "" }}{annotation}{tail}\n───╯"
}

pure threshold_location(argv: List[Str]) -> ValueLocation? {
  var found: ValueLocation? = null
  var index = 0
  while index < argv.len() {
    let raw = argv[index]
    if raw == "--" { return found }
    if raw == "-t" or raw == "--threshold" {
      found = if index + 1 < argv.len() { {index: index + 1, offset: 0} } else { null }
      index += 2
      continue
    }
    if raw.starts_with("--") {
      let equal = raw.find("=")
      let equal_at = equal ?? 0
      let name = if equal == null { raw.byte_slice(2) } else { raw.byte_slice(2, length: equal_at - 2) }
      if name != "" and "threshold".starts_with(name) {
        if equal != null { found = {index: index, offset: equal_at + 1} } else {
          found = if index + 1 < argv.len() { {index: index + 1, offset: 0} } else { null }
          index += 2
          continue
        }
      }
    } else if raw.starts_with("-t") and raw != "-t" {
      found = {index: index, offset: 2}
    }
    index += 1
  }
  found
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
          if size <= 0 { report_size_error("du", argv, found, value, disk.size_error(value, "block-size")); exit 1 }
        } else {
          report_size_error("du", argv, found, value, disk.size_error(value, "block-size"))
          exit 1
        }
      }
    }
    index += 1
  }
}

pure glob_regex(pattern: Str) -> Str {
  let chars = [ch for ch in pattern]
  let total = chars.len()
  var out = "(^|.*/)"
  var at = 0

  while at < total {
    let ch = chars[at]
    at += 1

    if ch == "*" {
      out = f"{out}(?s:.*)"
    } else if ch == "?" {
      out = f"{out}(?s:.)"
    } else if ch == "\\" and at < total {
      out = f"{out}{regex_escape(chars[at])}"
      at += 1
    } else if ch == "[" {
      var close = at

      if close < total and (chars[close] == "!" or chars[close] == "^") {
        close += 1
      }

      if close < total and chars[close] == "]" {
        close += 1
      }

      while close < total and chars[close] != "]" {
        close += 1
      }

      if close >= total {
        out = f"{out}["
      } else {
        var body = ""
        var inner = at

        if chars[inner] == "!" or chars[inner] == "^" {
          body = "^"
          inner += 1
        }

        while inner < close {
          let c = chars[inner]
          inner += 1

          if c == "[" and inner < close and chars[inner] == ":" {
            var end = inner

            while end < close and chars[end] != "]" {
              end += 1
            }

            body = f"{body}[{chars[inner..end].join("")}]"
            inner = end + 1
          } else if c == "\\" or c == "[" or c == "&" or c == "~" or c == "|" {
            body = f"{body}\\{c}"
          } else if c == "]" and inner == at + 1 {
            body = f"{body}\\]"
          } else {
            body = f"{body}{c}"
          }
        }

        out = f"{out}[{body}]"
        at = close + 1
      }
    } else {
      out = f"{out}{regex_escape(ch)}"
    }
  }

  f"{out}$"
}

pure regex_escape(ch: Str) -> Str {
  if ".+(){}|^$*?[]\\-".find(ch) != null { f"\\{ch}" } else { ch }
}

proc emit_usage(name: Str, size: Int, latest: Int, policy: Policy) [process, env, io, time, error] {
  if let threshold = policy.threshold {
    return when (! policy.negative and size < threshold) or (policy.negative and size > threshold)
  }
  let units = if policy.inodes { {...policy.units, block: 1} } else { policy.units }
  let timestamp = if policy.time == "" { "" } else { f"\t{time.format(latest, policy.time_style)?}" }
  var amount = disk.amount(size, units)
  if units.human != 0 {
    let locale = env.get_or("LC_ALL", env.get_or("LC_NUMERIC", env.get_or("LANG", "C") ?? "C") ?? "C") ?? "C"
    if locale.starts_with("fr") or locale.starts_with("de") or locale.starts_with("es") or locale.starts_with("it") or locale.starts_with("pt") {
      amount = amount.replace(".", with: ",")
    }
  }
  gnu.write_text(f"{amount}{timestamp}\t{name}{policy.ending}")
}

proc entry_metadata(root: FsRoot, entry: Path, name: Str, follow: Bool) [fs, error] -> Result[EntryMetadata, Error] {
  match root.stat(entry, follow_symlinks: follow) {
    Ok(meta) => Ok({meta: meta, external_target: null})
    Err(failure) => {
      return Err(failure) when ! follow
      match root.stat(entry, follow_symlinks: false) {
        Ok(link) => {
          return Err(failure) when link.kind != "symlink"
          let target = root.readlink(entry)?.display()
          let resolved = if target.starts_with("/") { fp"{target}" } else {
            let directory = root.host_path()?.display()
            fp"{if directory.ends_with("/") { f"{directory}{target}" } else { f"{directory}/{target}" }}"
          }
          match fs.stat(resolved, follow_symlinks: true) {
            Ok(meta) => Ok({meta: meta, external_target: resolved})
            Err(target_failure) => Err(target_failure)
          }
        }
        Err(_) => Err(failure)
      }
    }
  }
}

pure children_failure(errno: Int?, state: Str) -> Str {
  if let code = errno {
    match code {
      1 => "Operation not permitted"
      2 => "No such file or directory"
      5 => "Input/output error"
      9 => "Bad file descriptor"
      11 => "Resource temporarily unavailable"
      13 => "Permission denied"
      20 => "Not a directory"
      24 => "Too many open files"
      36 => "File name too long"
      40 => "Too many levels of symbolic links"
      else => f"OS error {code}"
    }
  } else if state == "truncated" {
    "directory entry limit exceeded"
  } else {
    state
  }
}

# Account in bytes until printing, and track device/inode identity across all
# operands. A link contributes once unless the caller requests link counting.
proc disk_usage(root: FsRoot, entry: Path, name: Str, policy: Policy, depth: Int, device: Int, links: Map[Bool], ancestors: List[Str]) [fs, process, env, io, time, error] -> Usage {
  for pattern in policy.excludes {
    if pattern.matches(name) {
      if policy.verbose { gnu.write_text(f"{gnu.quote(name)} ignored\n") }
      return {size: 0, accounted: 0, counted: false, links: links, latest: 0, failed: false, directory: false}
    }
  }
  let follow = policy.follow_all or (policy.follow_args and depth == 0) or name.ends_with("/")
  guard let found = entry_metadata(root, entry, name, follow) else { |failure|
    gnu.cannot_access(name, failure)
    return {size: 0, accounted: 0, counted: false, links: links, latest: 0, failed: true, directory: false}
  }
  let meta = found.meta
  let identity = f"{meta.dev}:{meta.ino}"
  if meta.kind == "dir" and identity in ancestors {
    if ! policy.follow_all { gnu.error(f"WARNING: Circular directory structure at {gnu.quote(name)}") }
    return {size: 0, accounted: 0, counted: false, links: links, latest: 0, failed: ! policy.follow_all, directory: true}
  }
  return {size: 0, accounted: 0, counted: false, links: links, latest: 0, failed: false, directory: meta.kind == "dir"} when ! policy.count_links and identity in links
  return {size: 0, accounted: 0, counted: false, links: links, latest: 0, failed: false, directory: meta.kind == "dir"} when policy.one_file_system and depth > 0 and meta.dev != device
  var seen = links
  seen[identity] = true
  var size = if policy.inodes { 1 } else if policy.apparent { if meta.kind in ["file", "symlink"] { meta.size } else { 0 } } else { meta.blocks_512 * 512 }
  var accounted = size
  var latest = if policy.time == "atime" { meta.atime_ns } else if policy.time == "ctime" { meta.ctime_ns } else { meta.mtime_ns }
  var failed = false
  if meta.kind == "dir" {
    let opened = if let host_path = found.external_target { fs.open_root(host_path) } else { root.open_root(entry) }
    match opened {
      Ok(child_root) => {
        defer child_root.close()
        match child_root.children(p".") {
          Ok(listing) => {
            for child in listing.children {
              let base = child.basename()
              let child_name = if name.ends_with("/") { f"{name}{base}" } else { f"{name}/{base}" }
              let result = disk_usage(child_root, child, child_name, policy, depth + 1, if depth == 0 { meta.dev } else { device }, seen, ancestors.push(identity))
              seen = result.links
              accounted += result.accounted
              if ! policy.separate or ! result.directory {
                size += result.size
                if result.counted and result.latest > latest { latest = result.latest }
              }
              failed = failed or result.failed
            }
            if ! listing.enumeration_succeeded {
              gnu.error(f"cannot read directory {gnu.quote(name)}: {children_failure(listing.errno, listing.state)}")
              failed = true
            }
          }
          Err(failure) => {
            gnu.error(f"cannot read directory {gnu.quote(name)}: {gnu.strerror(failure)}")
            failed = true
          }
        }
      }
      Err(failure) => {
        gnu.error(f"cannot read directory {gnu.quote(name)}: {gnu.strerror(failure)}")
        failed = true
      }
    }
  }
  if depth <= policy.depth and (meta.kind == "dir" or policy.all or depth == 0) {
    emit_usage(name, size, latest, policy)
  }
  {size: size, accounted: accounted, counted: true, links: seen, latest: latest, failed: failed, directory: meta.kind == "dir"}
}

proc disk_usage_operand(target: Path, policy: Policy, links: Map[Bool]) [fs, process, env, io, time, error] -> Usage {
  let name = target.display()
  let component = target.basename()
  let entry = if component == "/" { p"." } else { fp"{component}" }
  match fs.open_root(target.parent()) {
    Ok(root) => {
      defer root.close()
      disk_usage(root, entry, name, policy, 0, 0, links, [])
    }
    Err(failure) => {
      gnu.cannot_access(name, failure)
      {size: 0, accounted: 0, counted: false, links: links, latest: 0, failed: true, directory: false}
    }
  }
}

proc main(...argv: List[Str]) [fs, process, env, io, time, error] {
  if argv.len() > 0 and argv[-1] == "--threshold" {
    gnu.error("error: a value is required for '--threshold <SIZE>' but none was supplied\nFor more information, try '--help'.")
    exit 1
  }
  let opts: DuOptions = cli.applet(argv, {
    gnu: {status: 1},
    summarize: {form: "-s --summarize", default: false}, all: {form: "-a --all", default: false}, total: {form: "-c --total", default: false},
    apparent: {form: "-A --apparent-size", default: false}, bytes: {form: "-b --bytes", default: false},
    human: {form: "-h --human-readable", default: false}, si: {form: "--si", default: false},
    kilo: {form: "-k", default: false}, mega: {form: "-m", default: false}, block: {form: "-B --block-size SIZE", repeated: true},
    depth: {form: "-d --max-depth N"}, count_links: {form: "-l --count-links", default: false},
    follow_all: {form: "-L --dereference", default: false, conflicts: ["follow_args", "no_follow"]},
    follow_args: {form: "-D -H --dereference-args", default: false, conflicts: ["follow_all", "no_follow"]},
    no_follow: {form: "-P --no-dereference", default: false, conflicts: ["follow_all", "follow_args"]},
    separate: {form: "-S --separate-dirs", default: false}, one_file_system: {form: "-x --one-file-system", default: false},
    inodes: {form: "--inodes", default: false}, zero: {form: "-0 --null", default: false}, threshold: {form: "-t --threshold SIZE"},
    verbose: {form: "-v --verbose", default: false},
    excludes: {form: "--exclude PATTERN", repeated: true}, exclude_files: {form: "-X --exclude-from FILE", repeated: true},
    files0: {form: "--files0-from FILE"}, time: {form: "--time[=WORD]", optional_default: "mtime"}, time_style: {form: "--time-style STYLE"},
    help: {form: "--help", default: false, stop: true}, version: {form: "--version", default: false, stop: true},
    targets: {form: "...FILE"},
  })?
  if opts.help {
    gnu.help("Usage: du [OPTION]... [FILE]...\nSummarize disk usage.\n  -k, -m  use KiB or MiB display blocks\n  -b, --bytes  display apparent sizes in bytes\n  -a, --all  print files as well as directories\n  -s, --summarize  print only each operand\n  -l, --count-links  count hard links separately\n  -L, --dereference  follow every symbolic link\n  -H, --dereference-args  follow argument symbolic links\n  -d, --max-depth=N  limit printed directory depth\n  -x, --one-file-system  skip other filesystems\n  -c, --total  show a grand total\n  -S, --separate-dirs  exclude subdirectory totals from their parents\n  -B, --block-size=SIZE  select display units\n      --inodes  count filesystem objects\n      --exclude=PATTERN  omit matching paths\n      --files0-from=FILE  read NUL-separated operands\n  -t, --threshold=SIZE  filter printed usage values\n      --time[=WORD]  show latest modification, access, or status time\n      --time-style=STYLE  select timestamp formatting\n  -0, --null  terminate records with NUL\n")
    return
  }
  if opts.version { gnu.version("du")
    return }
  if opts.all and opts.summarize { gnu.usage_error("cannot both summarize and show all entries") }
  var depth = 9223372036854775807
  if let raw = opts.depth {
    if ! rx"^[0-9]+$".matches(raw) { gnu.usage_error(f"invalid maximum depth {gnu.quote_value(raw)}") }
    depth = raw.parse_int()?
  }
  if opts.summarize {
    if opts.depth != null and depth != 0 { gnu.usage_error("summarizing conflicts with --max-depth") }
    depth = 0
  }
  var threshold: Int? = null
  var negative = false
  if let raw = opts.threshold {
    negative = raw.starts_with("-")
    let unsigned = if raw.starts_with("-") or raw.starts_with("+") { raw.byte_slice(1) } else { raw }
    threshold = disk.parse_size(unsigned)
    if threshold == null or (negative and threshold == 0) {
      let message = disk.size_error(unsigned, "threshold").replace(gnu.quote_value(unsigned), with: gnu.quote_value(raw))
      if let location = threshold_location(argv) {
        report_size_error("du", argv, location, raw, message)
      } else {
        gnu.error(message)
      }
      exit 1
    }
  }
  validate_block_sizes(argv)
  let units = disk.argument_units(argv, disk.environment_units("du"), ["B", "block-size", "d", "max-depth", "t", "threshold", "X", "exclude-from", "exclude", "files0-from", "time-style"])
  var excludes = opts.excludes
  var failed = false
  for filename in opts.exclude_files {
    match fp"{filename}".read_text() {
      Ok(text) => { excludes = excludes.extend(text.lines().collect()) }
      Err(failure) => { gnu.error(gnu.strerror(failure))
        failed = true }
    }
  }
  var patterns: List[Regex] = []
  for pattern in excludes {
    match regex.compile(glob_regex(pattern)) {
      Ok(compiled) => patterns += [compiled]
      Err(_) => { gnu.error(f"Invalid exclude syntax: {gnu.quote(pattern)}")
        exit 1 }
    }
  }
  var time_field = ""
  if let choice = opts.time {
    let fields: Map[Str] = {mtime: "mtime", modification: "mtime", atime: "atime", access: "atime", use: "atime", ctime: "ctime", status: "ctime"}
    for alias in ["mtime", "modification", "atime", "access", "use", "ctime", "status"] {
      if choice != "" and alias.starts_with(choice) { time_field = fields[alias] }
    }
    if time_field == "" { gnu.usage_error(f"invalid argument {gnu.quote_value(choice)} for 'time'") }
  }
  var style = ""
  if time_field != "" {
    let environment_style = env.get_or("TIME_STYLE", "long-iso") ?? "long-iso"
    style = opts.time_style ?? environment_style
    if opts.time_style == null {
      if style == "locale" { style = "long-iso" }
      while style.starts_with("posix-") { style = style.byte_slice(6) }
      if style.starts_with("+") { style = style.split("\n")[0] }
    }
    if style.starts_with("+") { style = style.byte_slice(1) } else if style == "long-iso" { style = "%Y-%m-%d %H:%M" } else if style == "full-iso" { style = "%Y-%m-%d %H:%M:%S.%N %z" } else if style == "iso" { style = "%Y-%m-%d" } else { gnu.usage_error(f"invalid argument {gnu.quote_value(style)} for 'time style'") }
  }
  if opts.inodes and (opts.apparent or opts.bytes) {
    gnu.error("warning: options --apparent-size and -b are ineffective with --inodes")
  }
  var targets = opts.targets
  if let list = opts.files0 {
    if ! targets.is_empty() { gnu.error("file operands cannot be combined with --files0-from")
      exit 1 }
    guard let data = gnu.read_operand(list) else { |failure|
      gnu.error(f"{gnu.quote_maybe(list)}: read error: {gnu.strerror(failure)}")
      exit 1
    }
    guard let text = data.utf8() else { gnu.usage_error("file names must be valid UTF-8")
      return }
    targets = text.split("\0")
    if ! targets.is_empty() and targets[-1] == "" { targets = targets[..targets.len() - 1] }
    var valid: List[Str] = []
    for index in range(targets.len()) {
      let name = targets[index]
      if name == "" {
        gnu.error(f"{gnu.quote_maybe(list)}:{index + 1}: invalid zero-length file name")
        failed = true
      } else if name == "-" and list == "-" {
        gnu.error("when reading file names from standard input, no file name of '-' allowed")
        failed = true
      } else { valid += [name] }
    }
    targets = valid
  } else if targets.is_empty() { targets = ["."] }
  let policy: Policy = {all: opts.all, total: opts.total, apparent: opts.apparent or opts.bytes, count_links: opts.count_links, follow_all: opts.follow_all, follow_args: opts.follow_args, separate: opts.separate, one_file_system: opts.one_file_system, inodes: opts.inodes, depth: depth, threshold: threshold, negative: negative, units: units, excludes: patterns, ending: if opts.zero { "\0" } else { "\n" }, time: time_field, time_style: style, verbose: opts.verbose}
  var links: Map[Bool] = {}
  var total = 0
  var latest = 0
  var timestamp_seen = false
  for name in targets {
    let result = disk_usage_operand(fp"{name}", policy, links)
    links = result.links
    total += result.accounted
    if result.counted and (! timestamp_seen or result.latest > latest) { latest = result.latest
      timestamp_seen = true }
    failed = failed or result.failed
  }
  if opts.total { emit_usage("total", total, latest, policy) }
  if failed { exit 1 }
}
