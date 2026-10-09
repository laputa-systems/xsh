#!/bin/xsh
use lib.gnu
error AppletError = Usage(message: Str) : Usage

type DuOptions = {summarize: Bool, all: Bool, human: Bool, si: Bool, bytes_output: Bool, apparent_size: Bool, inodes: Bool, count_links: Bool, separate_dirs: Bool, one_file_system: Bool, null_output: Bool, max_depth: Int?, threshold: Int?, threshold_negative: Bool, dereference: Bool, dereference_args: Bool, unit: Int}
type DuSizeParse = {value: Int, issue: Str}
type DuMeta = {kind: Str, size: Int, blocks_512: Int, dev: Int, ino: Int}
type DuWalk = {size: Int, seen: List[Str]}

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

pure parse_size(text: Str) -> DuSizeParse {
  var number = text
  var selected = ""
  var factor = 1
  for candidate in ["YiB", "ZiB", "YB", "ZB", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB", "KB", "MB", "GB", "TB", "PB", "EB", "kB", "K", "M", "G", "T", "P", "E", "Y", "Z", "B"] {
    if text.ends_with(candidate) {
      selected = candidate
      number = text.byte_slice(0, length: text.byte_len() - candidate.byte_len())
      if candidate in ["YiB", "ZiB", "YB", "ZB", "Y", "Z"] { return {value: 0, issue: "too large"} }
      factor = if candidate in ["KB", "kB"] { 1000 } else if candidate == "MB" { 1000000 } else if candidate == "GB" { 1000000000 } else if candidate == "TB" { 1000000000000 } else if candidate == "PB" or candidate == "EB" { 1000000000000000 } else if candidate == "K" or candidate == "KiB" { 1024 } else if candidate == "M" or candidate == "MiB" { 1048576 } else if candidate == "G" or candidate == "GiB" { 1073741824 } else if candidate == "T" or candidate == "TiB" { 1099511627776 } else if candidate == "P" or candidate == "PiB" or candidate == "EiB" { 1125899906842624 } else { 1 }
      break
    }
  }
  let binary = number.starts_with("0b")
  if number.starts_with("0x") { return {value: 0, issue: "invalid"} }
  let digits = if binary { number.byte_slice(2) } else { number }
  let value: Int? = if digits == "" and selected != "" { 1 } else { parse_digits(digits, if binary { 2 } else { 10 }) }
  if value == null {
    let starts_numeric = number.byte_slice(0, length: 1) in ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"]
    return {value: 0, issue: if number == "" or number == "0b" or ! starts_numeric { "invalid" } else { "suffix"} }
  }
  let parsed = value ?? 0
  if parsed < 0 { return {value: 0, issue: "too large"} }
  if parsed == 0 { return {value: 0, issue: if selected == "B" { "suffix" } else { "invalid"} } }
  if parsed > 9007199254740 or parsed * factor > 9007199254740991 { return {value: 0, issue: "too large"} }
  {value: parsed * factor, issue: ""}
}

pure ceil_div(value: Int, unit: Int) -> Int {
  return 0 when value == 0

  (value + unit - 1) / unit
}

pure entry_size(meta: DuMeta, opts: DuOptions) -> Int {
  return 1 when opts.inodes
  if opts.bytes_output or opts.apparent_size {
    return 0 when meta.kind == "dir"

    return meta.size
  }

  meta.blocks_512 * 512
}

pure size_label(size_bytes: Int, opts: DuOptions) -> Str {
  return human_size(size_bytes, opts.si) when opts.human
  return f"{size_bytes}" when opts.inodes
  return f"{size_bytes}" when opts.unit == 1
  f"{ceil_div(size_bytes, opts.unit)}"
}

pure human_size(size_bytes: Int, decimal: Bool) -> Str {
  let base = if decimal { 1000 } else { 1024 }
  let suffixes = if decimal { ["k", "M", "G", "T", "P", "E"] } else { ["K", "M", "G", "T", "P", "E"] }
  return "0" when size_bytes == 0
  return f"{size_bytes}B" when size_bytes < base
  var value = size_bytes
  var index = 0
  while value >= base and index < suffixes.len() { value = value / base; index += 1 }
  var denominator = 1
  for _ in range(index) { denominator *= base }
  let remainder = size_bytes % denominator
  var tenths = 0
  if value >= 10 {
    if remainder * 2 >= denominator { value += 1 }
  } else {
    tenths = (remainder * 10 + denominator / 2) / denominator
    if tenths >= 10 { value += 1; tenths = 0 }
  }
  let shown = if value >= 10 { f"{value}" } else { f"{value}.{tenths}" }
  f"{shown}{suffixes[index - 1]}"
}

proc write_du_line(line: Str, null_output: Bool) [process, env, io] {
  if null_output { gnu.write_bytes(bytes.from_text(f"{line}\0")) } else { print $line }
}

proc disk_usage(target: Path, shown: Str, opts: DuOptions, top_level: Bool, depth: Int, seen: List[Str]) [fs, error, process, env, io] -> Result[DuWalk] {
  let follow = opts.dereference or (opts.dereference_args and top_level)
  let meta: DuMeta = fs.stat(target, follow_symlinks: follow)?
  let key = f"{meta.dev}:{meta.ino}"
  let duplicate = key in seen and ! opts.count_links
  var updated_seen = if duplicate or opts.count_links { seen } else { seen + [key] }
  var size = if duplicate { 0 } else { entry_size(meta, opts) }

  if meta.kind == "dir" and ! duplicate {
    for child in fs.children(target, ordered: false) {
      let child_shown = if shown.ends_with("/") { f"{shown}{child.name}" } else { f"{shown}/{child.name}" }
      let child_meta: DuMeta = fs.stat(child.path, follow_symlinks: opts.dereference)?
      if ! opts.one_file_system or child_meta.dev == meta.dev {
        let result = disk_usage(child.path, child_shown, opts, false, depth + 1, updated_seen)?
        updated_seen = result.seen
        if ! opts.separate_dirs or child_meta.kind != "dir" { size += result.size }
      }
    }
  }

  let passes_threshold = opts.threshold == null or (opts.threshold_negative and size <= (opts.threshold ?? 0)) or (! opts.threshold_negative and size >= (opts.threshold ?? 0))
  let within_depth = opts.max_depth == null or depth <= (opts.max_depth ?? 0)
  if ! duplicate and ! opts.summarize and passes_threshold and within_depth and (opts.all or meta.kind == "dir" or (top_level and meta.kind == "file")) {
    write_du_line(f"{size_label(size, opts)}\t{shown}", opts.null_output)
  }

  {size: size, seen: updated_seen}
}

type DuCliOptions = {
  summarize: Bool,
  human: Bool,
  all: Bool,
  total: Bool,
  bytes_output: Bool,
  apparent_size: Bool,
  kilobytes: Bool,
  megabytes: Bool,
  si: Bool,
  block_size: Str?,
  inodes: Bool,
  count_links: Bool,
  separate_dirs: Bool,
  one_file_system: Bool,
  max_depth: Str?,
  threshold: Str?,
  dereference: Bool,
  dereference_args: Bool,
  no_dereference: Bool,
  null_output: Bool,
  help: Bool,
  version: Bool,
  files0_from: Str?,
  targets: List[Str],
}

proc main(...argv: List[Str]) [fs, error, io, env, process] {
  if (argv.get(argv.len() - 1) ?? "") == "--threshold" or (argv.get(argv.len() - 1) ?? "") == "-t" {
    eprint "du: error: a value is required for '--threshold <SIZE>' but none was supplied"
    eprint "For more information, try '--help'."
    exit 1
  }
  let cli_opts: DuCliOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      summarize: {
        form: "-s --summarize",
        default: false,
      },
      human: {
        form: "-h --human-readable",
        default: false,
      },
      all: {
        form: "-a --all",
        default: false,
      },
      total: {
        form: "-c --total",
        default: false,
      },
      bytes_output: {
        form: "-b --bytes",
        default: false,
      },
      apparent_size: {
        form: "-A --apparent-size",
        default: false,
      },
      kilobytes: {
        form: "-k --kilobytes",
        default: false,
      },
      megabytes: {
        form: "-m",
        default: false,
      },
      si: {
        form: "--si",
        default: false,
      },
      block_size: {
        form: "-B --block-size SIZE",
      },
      inodes: {form: "--inodes", default: false},
      count_links: {form: "-l --count-links", default: false},
      separate_dirs: {form: "-S --separate-dirs", default: false},
      one_file_system: {form: "-x --one-file-system", default: false},
      max_depth: {form: "-d --max-depth DEPTH"},
      threshold: {form: "-t --threshold SIZE"},
      dereference: {form: "-L --dereference", default: false},
      dereference_args: {form: "-D -H --dereference-args", default: false},
      no_dereference: {form: "-P --no-dereference", default: false},
      null_output: {form: "-0 --null", default: false},
      help: {
        form: "--help",
        default: false,
        stop: true,
      },
      version: {
        form: "--version",
        default: false,
        stop: true,
      },
      files0_from: {form: "--files0-from FILE"},
      targets: {
        form: "...PATH",
      },
    },
  )?
  let {summarize, human, all, total, bytes_output, apparent_size, kilobytes, megabytes, si, block_size, inodes, count_links, separate_dirs, one_file_system, max_depth, threshold, dereference, dereference_args, no_dereference, null_output, help, version, files0_from, ..} = cli_opts

  if help {
    gnu.help("""Usage: du [OPTION]... [FILE]...
Summarize disk usage of the set of FILEs, recursively for directories.

  -a, --all             write counts for all files, not just directories
  -b, --bytes           equivalent to --apparent-size --block-size=1
  -c, --total           produce a grand total
  -h, --human-readable  print sizes in human readable format
  -k, --kilobytes       default to 1024-byte blocks
  -m                    default to 1 MiB blocks
  -d, --max-depth=N     print the total for a directory only if it is N or fewer levels below the command line argument
  -l, --count-links     count sizes many times if hard linked
  -S, --separate-dirs   do not include size of subdirectories
  -x, --one-file-system skip directories on different file systems
      --inodes          list inode usage instead of block usage
  -t, --threshold=SIZE  exclude entries smaller than SIZE if positive, or larger if negative
  -L, --dereference     follow all symbolic links
  -D, --dereference-args follow only command line symbolic links
  -P, --no-dereference do not follow any symbolic links
  -0, --null            end each output line with NUL, not newline
      --si              use powers of 1000
  -B, --block-size=SIZE scale sizes by SIZE before printing them
  -s, --summarize       display only a total for each argument
      --apparent-size   print apparent sizes rather than disk usage
      --help            display this help and exit
      --version         output version information and exit""")
    return
  }

  if version {
    gnu.version("du")
    return
  }
  if summarize and all { gnu.error("cannot both summarize and show all entries"); exit 1 }
  if inodes and (apparent_size or bytes_output) {
    gnu.error("warning: options --apparent-size and -b are ineffective with --inodes")
  }
  var targets = cli_opts.targets
  var failed = false

  if let list = files0_from {
    if targets.len() > 0 {
      gnu.error(f"extra operand {gnu.quote(targets[0])}")
      eprint "file operands cannot be combined with --files0-from"
      gnu.try_help()
      exit 1
    }

    var data = b""
    if list == "-" {
      data = io.stdin_bytes()?
    } else {
      if let Ok(metadata) = fs.stat(fp"{list}") {
        if metadata.kind == "dir" {
          gnu.error(f"{gnu.quote_maybe(list)}: read error: Is a directory")
          exit 1
        }
      }
      match fp"{list}".read_bytes() {
        Ok(read) => data = read
        Err(failure) => {
          if gnu.errno(failure) == 21 {
            gnu.error(f"{gnu.quote_maybe(list)}: read error: {gnu.strerror(failure)}")
          } else {
            gnu.cannot_open(list, failure)
          }
          exit 1
        }
      }
    }

    guard let text = data.utf8() else {
      gnu.error("file names that are not valid UTF-8 are not supported")
      exit 1
    }
    var names = text.split("\0")
    if names.len() > 0 and names[names.len() - 1] == "" { names = names[..names.len() - 1] }
    var name_index = 0
    for name in names {
      name_index += 1
      if name == "" {
        gnu.error(f"{gnu.quote_maybe(list)}:{name_index}: invalid zero-length file name")
        failed = true
      } else if name == "-" and list == "-" {
        gnu.error("when reading file names from standard input, no file name of '-' allowed")
        exit 1
      } else {
        targets += [name]
      }
    }
  } else if targets.len() == 0 {
    targets = ["."]
  }

  var unit = if env.get("POSIXLY_CORRECT") is Ok(_) { 512 } else { 1024 }
  for name in ["DU_BLOCK_SIZE", "BLOCK_SIZE", "BLOCKSIZE"] {
    if let Ok(value) = env.get(name) {
      if value != "" {
        let parsed = parse_size(value)
        if parsed.issue == "" and parsed.value > 0 { unit = parsed.value }
      }
      break
    }
  }
  if let requested = block_size {
    let parsed = parse_size(requested)
    if parsed.issue != "" {
      if parsed.issue == "too large" { gnu.error(f"--block-size argument {gnu.quote(requested)} too large") } else if parsed.issue == "suffix" { gnu.error(f"invalid suffix in --block-size argument {gnu.quote(requested)}") } else { gnu.error(f"invalid --block-size argument {gnu.quote(requested)}") }
      exit 1
    }
    unit = parsed.value
  }
  var apparent = apparent_size or bytes_output
  var human_output = false
  var si_output = false
  var index = 0
  while index < argv.len() {
    let arg = argv[index]
    if arg == "--block-size" or arg == "-B" {
      let requested = argv.get(index + 1) ?? ""
      let parsed = parse_size(requested)
      if parsed.issue != "" {
        if parsed.issue == "too large" { gnu.error(f"--block-size argument {gnu.quote(requested)} too large") } else if parsed.issue == "suffix" { gnu.error(f"invalid suffix in --block-size argument {gnu.quote(requested)}") } else { gnu.error(f"invalid --block-size argument {gnu.quote(requested)}") }
        exit 1
      }
      unit = parsed.value
      human_output = false
      si_output = false
      index += 1
    } else if arg.starts_with("--block-size=") or (arg.starts_with("-B") and arg != "-B") {
      let requested = if arg.starts_with("--block-size=") { arg.byte_slice(13) } else { arg.byte_slice(2) }
      let parsed = parse_size(requested)
      if parsed.issue != "" {
        if parsed.issue == "too large" { gnu.error(f"--block-size argument {gnu.quote(requested)} too large") } else if parsed.issue == "suffix" { gnu.error(f"invalid suffix in --block-size argument {gnu.quote(requested)}") } else { gnu.error(f"invalid --block-size argument {gnu.quote(requested)}") }
        exit 1
      }
      unit = parsed.value
      human_output = false
      si_output = false
    } else if arg == "--bytes" or arg == "-b" { unit = 1; apparent = true; human_output = false; si_output = false } else if arg == "-k" or arg == "--kilobytes" { unit = 1024; human_output = false; si_output = false } else if arg == "-m" { unit = 1048576; human_output = false; si_output = false } else if arg == "--si" { unit = 1000; human_output = false; si_output = true } else if arg == "-h" or arg == "--human-readable" { human_output = true; si_output = false } else if arg.starts_with("-") and ! arg.starts_with("--") {
      for letter in range(arg.byte_len() - 1) {
        let flag = arg.byte_slice(letter + 1, length: 1)
        if flag == "b" { unit = 1; apparent = true; human_output = false; si_output = false }
        if flag == "k" { unit = 1024; human_output = false; si_output = false }
        if flag == "m" { unit = 1048576; human_output = false; si_output = false }
        if flag == "h" { human_output = true; si_output = false }
      }
    }
    index += 1
  }
  if unit <= 0 { unit = 1024 }
  var depth_limit: Int? = null
  if let raw_depth = max_depth {
    if let Ok(parsed_depth) = raw_depth.parse_int() {
      if parsed_depth < 0 { gnu.error(f"invalid maximum depth {gnu.quote(raw_depth)}"); exit 1 }
      depth_limit = parsed_depth
    } else { gnu.error(f"invalid maximum depth {gnu.quote(raw_depth)}"); exit 1 }
  }
  var threshold_size: Int? = null
  var threshold_negative = false
  if let raw_threshold = threshold {
    let has_sign = raw_threshold.starts_with("-") or raw_threshold.starts_with("+")
    threshold_negative = raw_threshold.starts_with("-")
    let magnitude = if has_sign { raw_threshold.byte_slice(1) } else { raw_threshold }
    let parsed_threshold = parse_size(magnitude)
    if parsed_threshold.issue != "" or parsed_threshold.value == 0 {
      if parsed_threshold.issue == "too large" { gnu.error(f"--threshold argument {gnu.quote(raw_threshold)} too large") } else if parsed_threshold.issue == "suffix" { gnu.error(f"invalid suffix in --threshold argument {gnu.quote(raw_threshold)}") } else { gnu.error(f"invalid --threshold argument {gnu.quote(raw_threshold)}") }
      exit 1
    }
    threshold_size = parsed_threshold.value
  }
  var grand_total = 0
  var follow_mode = if dereference { 1 } else if dereference_args { 2 } else { 0 }
  for arg in argv {
    if arg == "-L" or arg == "--dereference" { follow_mode = 1 }
    if arg == "-D" or arg == "-H" or arg == "--dereference-args" { follow_mode = 2 }
    if arg == "-P" or arg == "--no-dereference" { follow_mode = 0 }
  }
  let opts: DuOptions = {summarize: summarize, all: all, human: human_output, si: si_output or si, bytes_output: bytes_output, apparent_size: apparent, inodes: inodes, count_links: count_links, separate_dirs: separate_dirs, one_file_system: one_file_system, null_output: null_output, max_depth: depth_limit, threshold: threshold_size, threshold_negative: threshold_negative, dereference: follow_mode == 1, dereference_args: follow_mode == 2, unit: unit}
  var seen: List[Str] = []

  for item in targets {
    let target = fp"{item}"
    let result = disk_usage(target, item, opts, true, 0, seen)?
    seen = result.seen
    grand_total += result.size
    let result_size = result.size

    if summarize {
      if result_size > 0 { write_du_line(f"{size_label(result_size, opts)}\t{item}", null_output) }
    }
  }

  if total {
    write_du_line(f"{size_label(grand_total, opts)}\ttotal", null_output)
  }
  if failed { exit 1 }
}
