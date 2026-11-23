#!/bin/xsh
use lib.gnu as gnu
use lib.util_linux_io as ul_io

pure util_strerror(failure: Error) -> Str {
  let text = failure.message
  let segments = text.split(" (os error ")
  let at = if segments.len() > 1 { text.byte_len() - segments[-1].byte_len() - " (os error ".byte_len() } else { -1 }
  return text when at < 0
  let parts = text.byte_slice(0, at).split(": ")
  parts[-1]
}

const MAXSYMLINKS = 20
const USAGE = """Usage:
 namei [options] <pathname>...

Follow a pathname until a terminal point is found.

Options:
 -x, --mountpoints   show mount point directories with a 'D'
 -m, --modes         show the mode bits of each file
 -o, --owners        show owner and group name of each file
 -l, --long          use a long listing format (-m -o -v)\x20
 -n, --nosymlinks    don't follow symlinks
 -v, --vertical      vertical align of modes and owners
 -h, --help          display this help
 -V, --version       display version

For more details see namei(1).
"""

type Options = {
  mounts: Bool,
  modes: Bool,
  owners: Bool,
  long: Bool,
  nosymlinks: Bool,
  vertical: Bool,
  help: Bool,
  version: Bool,
  paths: List[Str],
}

type Entry = {
  name: Bytes,
  path: Bytes,
  level: Int,
  metadata: FsStat?,
  stat_error: Error?,
  link: Bytes?,
  link_error: Error?,
  mountpoint: Bool,
  mount_error: Error?,
}

type Expansion = {entries: List[Entry], width_entries: List[Entry], exceeded: Bool}
type OwnerWidths = {user_width: Int, group_width: Int}
type Listing = {chunks: List[Bytes], failed: Bool}
type Output = {buffer: Bytes, initialized: Bool, failed: Bool}

proc unrecognized_option(value: Bytes) [process, env, io] -> Unit {
  ul_io.write_diagnostic_bytes(bytes.concat([bytes.from_text(f"{gnu.prog()}: unrecognized option: "), value, b"\n"]))
  gnu.try_help()
  exit 1
}

proc parse_options(argv: List[Str], raw: List[gnu.RawArgument]) [process, env, io] -> Options {
  var options: Options = {mounts: false, modes: false, owners: false, long: false, nosymlinks: false, vertical: false, help: false, version: false, paths: []}
  var at = 0
  while at < argv.len() {
    let word = argv[at]
    at += 1
    return {...options, paths: options.paths.extend(argv[at..])} when word == "--"
    if ! word.starts_with("-") or word == "-" {
      options = {...options, paths: options.paths.extend([word])}
      continue
    }
    var flags: List[Str] = []
    if word.starts_with("--") {
      let body = word.byte_slice(2)
      let typed = body.split("=")[0]
      let names = ["mountpoints", "modes", "owners", "long", "nosymlinks", "vertical", "help", "version"]
      let exact = [name for name in names if name == typed]
      let matches = if exact.is_empty() { [name for name in names if name.starts_with(typed)] } else { exact }
      if matches.is_empty() { unrecognized_option(gnu.argument_bytes(body, raw)) }
      if matches.len() > 1 { gnu.usage_error(f"option is ambiguous: {body}") }
      if "=" in body { gnu.usage_error(f"option does not take an argument: {matches[0]}") }
      flags = matches
    } else {
      let marker_at = word.find("\0") ?? word.byte_len()
      flags = [flag for flag in word.byte_slice(1, length: marker_at - 1)]
      if marker_at < word.byte_len() { flags += [word.byte_slice(marker_at)] }
    }
    for flag in flags {
      match flag {
        "x" | "mountpoints" => options = {...options, mounts: true}
        "m" | "modes" => options = {...options, modes: true}
        "o" | "owners" => options = {...options, owners: true}
        "l" | "long" => options = {...options, long: true}
        "n" | "nosymlinks" => options = {...options, nosymlinks: true}
        "v" | "vertical" => options = {...options, vertical: true}
        "h" | "help" => return {...options, help: true}
        "V" | "version" => return {...options, version: true}
        "Z" => gnu.usage_error("unrecognized option: Z")
        else => {
          let value = gnu.argument_bytes(flag, raw)
          unrecognized_option(if flag.starts_with("\0") or (ul_io.byte_locale() and ! word.starts_with("--")) { value[..1] } else { value })
        }
      }
    }
  }
  options
}

pure has_bit(mode: Int, bit: Int) -> Bool {
  mode.bit_and(bit) != 0
}

pure mode_triplet(mode: Int, read_bit: Int, write_bit: Int, exec_bit: Int) -> Str {
  let read = if has_bit(mode, read_bit) { "r" } else { "-" }
  let write = if has_bit(mode, write_bit) { "w" } else { "-" }
  let execute = if has_bit(mode, exec_bit) { "x" } else { "-" }
  f"{read}{write}{execute}"
}

pure mode_string(metadata: FsStat, mountpoint: Bool) -> Str {
  let kind = match metadata.kind {
    "dir" => "d"
    "symlink" => "l"
    "char" => "c"
    "block" => "b"
    "fifo" => "p"
    "socket" => "s"
    else => "-"
  }
  var owner = mode_triplet(metadata.mode, 0o400, 0o200, 0o100)
  var group_bits = mode_triplet(metadata.mode, 0o40, 0o20, 0o10)
  var other = mode_triplet(metadata.mode, 0o4, 0o2, 0o1)
  if has_bit(metadata.mode, 0o4000) { owner = owner.byte_slice(0, length: 2) + (if has_bit(metadata.mode, 0o100) { "s" } else { "S" }) }
  if has_bit(metadata.mode, 0o2000) { group_bits = group_bits.byte_slice(0, length: 2) + (if has_bit(metadata.mode, 0o10) { "s" } else { "S" }) }
  if has_bit(metadata.mode, 0o1000) { other = other.byte_slice(0, length: 2) + (if has_bit(metadata.mode, 0o1) { "t" } else { "T" }) }
  f"{if mountpoint { "D" } else { kind }}{owner}{group_bits}{other}"
}

pure last_slash(pathname: Bytes) -> Int? {
  var found: Int? = null
  for index in range(pathname.len()) {
    if pathname.byte_at(index) == 47 { found = index }
  }
  found
}

proc inspect(name: Bytes, path_bytes: Bytes, level: Int, parent: Entry?, mounts: Bool) [fs, error] -> Result[Entry, Error] {
  let path_value = Path.parse_bytes(path_bytes)?
  var metadata: FsStat? = null
  var stat_error: Error? = null
  var link: Bytes? = null
  var link_error: Error? = null
  var mountpoint = false
  var mount_error: Error? = null

  match fs.stat(path_value, follow_symlinks: false) {
    Ok(value) => metadata = value
    Err(failure) => stat_error = failure
  }

  if let value = metadata {
    if value.kind == "symlink" {
      match path_value.readlink() {
        Ok(target) => link = target.bytes()
        Err(failure) => link_error = failure
      }
    }
    if mounts and value.kind == "dir" {
      var compare: FsStat? = null
      if let previous = parent {
        if let parent_metadata = previous.metadata {
          if parent_metadata.kind == "dir" {
            compare = parent_metadata
          } else if parent_metadata.kind == "symlink" {
            let parent_path = Path.parse_bytes(bytes.concat([path_bytes, b"/.."]))?
            match fs.stat(parent_path) {
              Ok(parent_stat) => compare = parent_stat
              Err(failure) => mount_error = failure
            }
          }
        }
      } else {
        let parent_path = Path.parse_bytes(bytes.concat([path_bytes, b"/.."]))?
        match fs.stat(parent_path) {
          Ok(parent_stat) => compare = parent_stat
          Err(failure) => mount_error = failure
        }
      }
      if let previous_metadata = compare {
        mountpoint = previous_metadata.dev != value.dev or previous_metadata.ino == value.ino
      }
    }
  }

  Ok({name: name, path: path_bytes, level: level, metadata: metadata, stat_error: stat_error, link: link, link_error: link_error, mountpoint: mountpoint, mount_error: mount_error})
}

proc path_entries(pathname: Bytes, start: Int, base: Bytes, level: Int, parent: Entry?, mounts: Bool) [fs, error] -> Result[List[Entry], Error] {
  var entries: List[Entry] = []
  var current = base
  var previous = parent
  var offset = start
  if offset < pathname.len() and pathname.byte_at(offset) == 47 {
    while offset < pathname.len() and pathname.byte_at(offset) == 47 { offset += 1 }
    let root = inspect(b"/", b"/", level, parent, mounts)?
    entries += [root]
    previous = root
    current = pathname[0..offset]
  } else if ! base.is_empty() and base != b"/" and start > 0 {
    current = bytes.concat([base, b"/"])
  }
  var begin = offset
  while begin < pathname.len() {
    var end = begin
    while end < pathname.len() and pathname.byte_at(end) != 47 { end += 1 }
    if end > begin {
      let component = pathname[begin..end]
      let full_path = bytes.concat([current, pathname[offset..end]])
      let item = inspect(component, full_path, level, previous, mounts)?
      entries += [item]
      previous = item
    }
    begin = end + 1
  }
  Ok(entries)
}

proc symlink_entries(entry: Entry, mounts: Bool) [fs, error] -> Result[List[Entry], Error] {
  guard let target = entry.link else { return Ok([]) }
  if target.starts_with(b"/") {
    return path_entries(target, 0, b"", entry.level + 1, entry, mounts)
  }
  if let slash = last_slash(entry.path) {
    let prefix = entry.path[0..slash]
    let base = if prefix.is_empty() { b"/" } else { prefix }
    let expanded = bytes.concat([base, if base == b"/" { b"" } else { b"/" }, target])
    let begin = base.len() + (if base == b"/" { 0 } else { 1 })
    return path_entries(expanded, begin, base, entry.level + 1, entry, mounts)
  }
  path_entries(target, 0, b"", entry.level + 1, entry, mounts)
}

proc expand(entries: List[Entry], nosymlinks: Bool, mounts: Bool) [fs, error] -> Result[Expansion, Error] {
  var expanded = entries
  var index = 0
  var links = 0
  while index < expanded.len() {
    let item = expanded[index]
    if ! nosymlinks and item.link != null {
      links += 1
      if links > MAXSYMLINKS {
        return Ok({entries: expanded[0..index + 1], width_entries: expanded, exceeded: true})
      }
      let inserted = symlink_entries(item, mounts)?
      expanded = expanded[0..index + 1] + inserted + expanded[index + 1..]
    }
    index += 1
  }
  Ok({entries: expanded, width_entries: expanded, exceeded: false})
}

const NONSPACING = [
  [768, 879], [1155, 1161], [1425, 1469], [1471, 1471], [1473, 1474], [1476, 1477],
  [1479, 1479], [1536, 1541], [1552, 1562], [1564, 1564], [1611, 1631], [1648, 1648],
  [1750, 1757], [1759, 1764], [1767, 1768], [1770, 1773], [1807, 1807], [1809, 1809],
  [1840, 1866], [1958, 1968], [2027, 2035], [2045, 2045], [2070, 2073], [2075, 2083],
  [2085, 2087], [2089, 2093], [2137, 2139], [2259, 2306], [2362, 2362], [2364, 2364],
  [2369, 2376], [2381, 2381], [2385, 2391], [2402, 2403], [2433, 2433], [2492, 2492],
  [2497, 2500], [2509, 2509], [2530, 2531], [2558, 2558], [2561, 2562], [2620, 2620],
  [2625, 2626], [2631, 2632], [2635, 2637], [2641, 2641], [2672, 2673], [2677, 2677],
  [2689, 2690], [2748, 2748], [2753, 2757], [2759, 2760], [2765, 2765], [2786, 2787],
  [2810, 2815], [2817, 2817], [2876, 2876], [2879, 2879], [2881, 2884], [2893, 2893],
  [2902, 2902], [2914, 2915], [2946, 2946], [3008, 3008], [3021, 3021], [3072, 3072],
  [3076, 3076], [3134, 3136], [3142, 3144], [3146, 3149], [3157, 3158], [3170, 3171],
  [3201, 3201], [3260, 3260], [3263, 3263], [3270, 3270], [3276, 3277], [3298, 3299],
  [3328, 3329], [3387, 3388], [3393, 3396], [3405, 3405], [3426, 3427], [3530, 3530],
  [3538, 3540], [3542, 3542], [3633, 3633], [3636, 3642], [3655, 3662], [3761, 3761],
  [3764, 3772], [3784, 3789], [3864, 3865], [3893, 3893], [3895, 3895], [3897, 3897],
  [3953, 3966], [3968, 3972], [3974, 3975], [3981, 3991], [3993, 4028], [4038, 4038],
  [4141, 4144], [4146, 4151], [4153, 4154], [4157, 4158], [4184, 4185], [4190, 4192],
  [4209, 4212], [4226, 4226], [4229, 4230], [4237, 4237], [4253, 4253], [4448, 4607],
  [4957, 4959], [5906, 5908], [5938, 5940], [5970, 5971], [6002, 6003], [6068, 6069],
  [6071, 6077], [6086, 6086], [6089, 6099], [6109, 6109], [6155, 6158], [6277, 6278],
  [6313, 6313], [6432, 6434], [6439, 6440], [6450, 6450], [6457, 6459], [6679, 6680],
  [6683, 6683], [6742, 6742], [6744, 6750], [6752, 6752], [6754, 6754], [6757, 6764],
  [6771, 6780], [6783, 6783], [6832, 6846], [6912, 6915], [6964, 6964], [6966, 6970],
  [6972, 6972], [6978, 6978], [7019, 7027], [7040, 7041], [7074, 7077], [7080, 7081],
  [7083, 7085], [7142, 7142], [7144, 7145], [7149, 7149], [7151, 7153], [7212, 7219],
  [7222, 7223], [7376, 7378], [7380, 7392], [7394, 7400], [7405, 7405], [7412, 7412],
  [7416, 7417], [7616, 7673], [7675, 7679], [8203, 8207], [8234, 8238], [8288, 8292],
  [8294, 8303], [8400, 8432], [11503, 11505], [11647, 11647], [11744, 11775], [12330, 12333],
  [12441, 12442], [42607, 42610], [42612, 42621], [42654, 42655], [42736, 42737], [43010, 43010],
  [43014, 43014], [43019, 43019], [43045, 43046], [43204, 43205], [43232, 43249], [43263, 43263],
  [43302, 43309], [43335, 43345], [43392, 43394], [43443, 43443], [43446, 43449], [43452, 43453],
  [43493, 43493], [43561, 43566], [43569, 43570], [43573, 43574], [43587, 43587], [43596, 43596],
  [43644, 43644], [43696, 43696], [43698, 43700], [43703, 43704], [43710, 43711], [43713, 43713],
  [43756, 43757], [43766, 43766], [44005, 44005], [44008, 44008], [44013, 44013], [55216, 55295],
  [64286, 64286], [65024, 65039], [65056, 65071], [65279, 65279], [65529, 65531], [66045, 66045],
  [66272, 66272], [66422, 66426], [68097, 68099], [68101, 68102], [68108, 68111], [68152, 68154],
  [68159, 68159], [68325, 68326], [68900, 68903], [69446, 69456], [69633, 69633], [69688, 69702],
  [69759, 69761], [69811, 69814], [69817, 69818], [69821, 69821], [69837, 69837], [69888, 69890],
  [69927, 69931], [69933, 69940], [70003, 70003], [70016, 70017], [70070, 70078], [70089, 70092],
  [70191, 70193], [70196, 70196], [70198, 70199], [70206, 70206], [70367, 70367], [70371, 70378],
  [70400, 70401], [70459, 70460], [70464, 70464], [70502, 70508], [70512, 70516], [70712, 70719],
  [70722, 70724], [70726, 70726], [70750, 70750], [70835, 70840], [70842, 70842], [70847, 70848],
  [70850, 70851], [71090, 71093], [71100, 71101], [71103, 71104], [71132, 71133], [71219, 71226],
  [71229, 71229], [71231, 71232], [71339, 71339], [71341, 71341], [71344, 71349], [71351, 71351],
  [71453, 71455], [71458, 71461], [71463, 71467], [71727, 71735], [71737, 71738], [72148, 72151],
  [72154, 72155], [72160, 72160], [72193, 72202], [72243, 72248], [72251, 72254], [72263, 72263],
  [72273, 72278], [72281, 72283], [72330, 72342], [72344, 72345], [72752, 72758], [72760, 72765],
  [72767, 72767], [72850, 72871], [72874, 72880], [72882, 72883], [72885, 72886], [73009, 73014],
  [73018, 73018], [73020, 73021], [73023, 73029], [73031, 73031], [73104, 73105], [73109, 73109],
  [73111, 73111], [73459, 73460], [78896, 78904], [92912, 92916], [92976, 92982], [94031, 94031],
  [94095, 94098], [113821, 113822], [113824, 113827], [119143, 119145], [119155, 119170], [119173, 119179],
  [119210, 119213], [119362, 119364], [121344, 121398], [121403, 121452], [121461, 121461], [121476, 121476],
  [121499, 121503], [121505, 121519], [122880, 122886], [122888, 122904], [122907, 122913], [122915, 122916],
  [122918, 122922], [123184, 123190], [123628, 123631], [125136, 125142], [125252, 125258],
]

pure account_codepoint(value: Str) -> Int {
  let raw = bytes.from_text(value)
  let first = raw.byte_at(0) ?? 0
  return first when raw.len() == 1
  let second = (raw.byte_at(1) ?? 128) - 128
  return (first - 192) * 64 + second when raw.len() == 2
  let third = (raw.byte_at(2) ?? 128) - 128
  return (first - 224) * 4096 + second * 64 + third when raw.len() == 3
  (first - 240) * 262144 + second * 4096 + third * 64 + (raw.byte_at(3) ?? 128) - 128
}

pure valid_account_name(name: Str, byte_locale: Bool) -> Bool {
  var visible = false
  for character in name {
    let cp = account_codepoint(character)
    return false when cp < 32 or cp == 127 or (! byte_locale and cp >= 128 and cp < 160)
    if byte_locale { visible = true; continue }
    return false when cp % 65536 >= 65534
    var zero = cp == 917505 or (cp >= 917536 and cp < 917631) or (cp >= 917760 and cp < 917999)
    if cp < 131072 {
      for bounds in NONSPACING {
        if cp >= bounds[0] and cp <= bounds[1] { zero = true; break }
      }
    }
    if ! zero { visible = true }
  }
  visible
}

proc username(uid: Int, byte_locale: Bool) [fs] -> Str {
  match user.by_uid(uid) {
    Ok(account) => if valid_account_name(account.name, byte_locale) { account.name } else { f"{uid}" }
    Err(_) => f"{uid}"
  }
}

proc groupname(gid: Int, byte_locale: Bool) [fs] -> Str {
  match group.by_gid(gid) {
    Ok(group_entry) => if valid_account_name(group_entry.name, byte_locale) { group_entry.name } else { f"{gid}" }
    Err(_) => f"{gid}"
  }
}

pure owner_width(name: Str, byte_locale: Bool) -> Int {
  let width = if byte_locale { name.byte_len() } else { tui.cell_width(name) }
  return width when width <= 256
  return 256 when byte_locale
  var used = 0
  for character in name {
    used += tui.cell_width(character)
    return used when used >= 256
  }
  used
}

proc update_owner_widths(entries: List[Entry], owners: Bool, user_width: Int, group_width: Int, byte_locale: Bool) [fs] -> OwnerWidths {
  var max_user = user_width
  var max_group = group_width
  if owners {
    for entry in entries {
      if let metadata = entry.metadata {
        let user_name = username(metadata.uid, byte_locale)
        let group_name = groupname(metadata.gid, byte_locale)
        if owner_width(user_name, byte_locale) > max_user { max_user = owner_width(user_name, byte_locale) }
        if owner_width(group_name, byte_locale) > max_group { max_group = owner_width(group_name, byte_locale) }
      }
    }
  }
  {user_width: max_user, group_width: max_group}
}

pure spaces(count: Int) -> Bytes {
  bytes.concat([b" " for _ in range(if count > 0 { count } else { 0 })])
}

proc render(entries: List[Entry], operand: Bytes, options: Options, user_width: Int, group_width: Int, byte_locale: Bool) [fs] -> Listing {
  return {chunks: [], failed: false} when entries.is_empty()
  var chunks: List[Bytes] = [b"f: ", operand, b"\n"]
  var failed = false
  for entry in entries {
    if let failure = entry.stat_error {
      failed = true
      let blanks = 1 + (if options.modes { 9 } else { 0 }) + (if options.owners { user_width + group_width + 2 } else { 0 }) + (if options.vertical { 0 } else { 1 }) + 1 + entry.level * 2
      chunks += [spaces(blanks), b" ", entry.name, b" - ", bytes.from_text(util_strerror(failure)), b"\n"]
      break
    }
    guard let metadata = entry.metadata else { continue }
    let mode = mode_string(metadata, entry.mountpoint)
    if ! options.vertical {
      for _ in range(entry.level) { chunks += [b"  "] }
      chunks += [b" "]
    }
    chunks += [if options.modes { bytes.from_text(mode) } else { bytes.from_text(mode.byte_slice(0, length: 1)) }]
    if options.owners {
      let user_name = username(metadata.uid, byte_locale)
      let group_name = groupname(metadata.gid, byte_locale)
      chunks += [b" ", bytes.from_text(user_name), spaces(user_width - user_name.byte_len()), b" ", bytes.from_text(group_name), spaces(group_width - group_name.byte_len())]
    }
    if options.vertical { for _ in range(entry.level) { chunks += [b"  "] } }
    if let target = entry.link {
      chunks += [b" ", entry.name, b" -> ", target, b"\n"]
    } else {
      chunks += [b" ", entry.name, b"\n"]
    }
  }
  {chunks: chunks, failed: failed}
}

proc write_output(output: Output, data: Bytes) [io] -> Output {
  return output when output.failed
  if let Err(_) = io.write_stdout_bytes(data) { return {...output, buffer: b"", initialized: true, failed: true} }
  if let Err(_) = io.flush_stdout() { return {...output, buffer: b"", initialized: true, failed: true} }
  {...output, buffer: b"", initialized: true}
}

proc output_chunks(initial: Output, chunks: List[Bytes], terminal: Bool) [io] -> Output {
  var output = initial
  for chunk in chunks {
    break when output.failed
    continue when chunk.is_empty()
    var newline = 0
    if terminal or ! output.initialized {
      for index in range(chunk.len()) {
        if chunk.byte_at(index) == 10 { newline = index + 1 }
      }
    }
    if output.buffer.len() + chunk.len() > 1024 {
      output = write_output(output, bytes.concat([output.buffer, chunk]))
    } else if newline > 0 {
      output = write_output(output, bytes.concat([output.buffer, chunk[0..newline]]))
      if ! output.failed { output = {...output, buffer: chunk[newline..]} }
    } else {
      output = {...output, buffer: bytes.concat([output.buffer, chunk])}
    }
  }
  output
}

proc report_fatal(pathname: Bytes, kind: Str, failure: Error, output: Output) [process, env, io, error] {
  ul_io.write_diagnostic_bytes(bytes.concat([
    bytes.from_text(f"{gnu.prog()}: {kind}"), pathname,
    bytes.from_text(f": {util_strerror(failure)}\n"),
  ]))
  let finished = if output.buffer.is_empty() { output } else { write_output(output, output.buffer) }
  if finished.failed { gnu.error("write error") }
  exit 1
}

proc report_loop(pathname: Bytes) [process, env, io] {
  ul_io.write_diagnostic_bytes(bytes.concat([bytes.from_text(f"{gnu.prog()}: "), pathname, b": exceeded limit of symlinks\n"]))
}

proc main(...argv: List[Bytes]) [fs, process, env, error, io] {
  if process.signal_action("PIPE")? == "default" {
    process.set_signal_action("PIPE", "default")
  }

  let prepared = gnu.prepare_arguments(argv)
  let options = parse_options(prepared.text, prepared.raw)

  if options.help { ul_io.help(f"\n{USAGE}"); return }
  if options.version { ul_io.version("namei"); return }
  if options.paths.is_empty() {
    gnu.error("pathname argument is missing")
    gnu.try_help()
    exit 1
  }

  let long = options.long
  let normalized_options = {...options, modes: options.modes or long, owners: options.owners or long, vertical: options.vertical or long}
  var failed = false
  var user_width = 0
  var group_width = 0
  var output: Output = {buffer: b"", initialized: false, failed: false}
  let byte_locale = ul_io.byte_locale()
  let terminal = unix.isatty(1)
  for token in normalized_options.paths {
    let operand = gnu.argument_bytes(token, prepared.raw)
    let operand_path = Path.parse_bytes(operand)?
    if fs.stat(operand_path, follow_symlinks: true) is Err(_) { failed = true }
    let entries_from_root = path_entries(operand, 0, b"", 0, null, options.mounts)?
    let result = expand(entries_from_root, options.nosymlinks, options.mounts)?
    for entry in result.width_entries {
      if let failure = entry.link_error { report_fatal(entry.path, "failed to read symlink: ", failure, output) }
      if let failure = entry.mount_error { report_fatal(bytes.concat([entry.path, b"/.. failed"]), "stat of ", failure, output) }
    }
    let widths = update_owner_widths(result.width_entries, normalized_options.owners, user_width, group_width, byte_locale)
    user_width = widths.user_width
    group_width = widths.group_width
    let listing = render(result.entries, operand, normalized_options, user_width, group_width, byte_locale)
    output = output_chunks(output, listing.chunks, terminal)
    failed = listing.failed or failed
    if result.exceeded and ! listing.failed {
      failed = true
      report_loop(operand)
    }
  }
  if ! output.buffer.is_empty() { output = write_output(output, output.buffer) }
  if output.failed { gnu.error("write error"); exit 1 }
  if failed { exit 1 }
}
