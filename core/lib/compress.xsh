##! Compression applets share byte streaming and publish files only after a
##! complete codec operation. Source removal follows successful publication.
use gnu

# `has_value` separates `--long=` (an empty value) from `--long`.
type OptionToken = {kind: Str, name: Str, value: Str, has_value: Bool}

# Short options that consume a value: the rest of the cluster or, when nothing
# follows, the next argument. zstd's -o takes only the next argument, so a
# glued `-ofile` reads as flags the way the reference tool does.
pure short_takes_value(format: Str, option: Str) -> Bool {
  if format in ["xz", "lzma"] { option in ["T", "S", "F", "C"] } else if format == "gz" { option == "S" } else { format == "zstd" and option in ["o", "D"] }
}

# Long options that take their value from the next argument when no `=` is
# given. Options with only an optional value (`--long`, `--fast`) need `=`.
pure long_takes_value(format: Str, option: Str) -> Bool {
  if format in ["xz", "lzma"] { option in ["threads", "block-size", "suffix", "format", "check"] } else if format == "gz" { option == "suffix" } else { format == "zstd" and option == "threads" }
}

pure digit(text: Str) -> Bool { "0123456789".find(text) != null }

# Compression levels begin with digits, which generic flag tokenization treats
# as negative-number operands. Interpret them as options until an explicit --.
pure option_tokens(argv: List[Str], format: Str) -> List[OptionToken] {
  var operands_only = false
  var next_arg = 0
  collect {
    while next_arg < argv.len() {
      let arg = argv[next_arg]
      next_arg += 1
      if ! operands_only and arg == "--" { operands_only = true; continue }
      if operands_only or arg == "-" or ! arg.starts_with("-") {
        yield {kind: "operand", name: arg, value: "", has_value: false}
      } else if arg.starts_with("--") {
        let raw = arg.byte_slice(2)
        let equals = raw.find("=")
        if equals != null {
          yield {kind: "long", name: raw.byte_slice(0, length: equals), value: raw.byte_slice(equals + 1), has_value: true}
        } else if long_takes_value(format, raw) and next_arg < argv.len() {
          yield {kind: "long", name: raw, value: argv[next_arg], has_value: true}
          next_arg += 1
        } else {
          yield {kind: "long", name: raw, value: "", has_value: false}
        }
      } else {
        var at = 1
        while at < arg.byte_len() {
          let start = at
          at += 1
          let letter = arg.byte_slice(start, length: 1)
          # zstd levels and its -T and -B counts are digit runs glued to the flag.
          if format == "zstd" and (digit(letter) or letter in ["T", "B"]) {
            let leading = digit(letter)
            let digits_from = if leading { start } else { at }
            while at < arg.byte_len() and digit(arg.byte_slice(at, length: 1)) { at += 1 }
            let digits = arg.byte_slice(digits_from, length: at - digits_from)
            if leading {
              yield {kind: "short", name: digits, value: "", has_value: false}
            } else {
              yield {kind: "short", name: letter, value: digits, has_value: digits != ""}
            }
          } else if short_takes_value(format, letter) {
            if at < arg.byte_len() and format != "zstd" {
              yield {kind: "short", name: letter, value: arg.byte_slice(at), has_value: true}
              at = arg.byte_len()
            } else if at >= arg.byte_len() and next_arg < argv.len() {
              yield {kind: "short", name: letter, value: argv[next_arg], has_value: true}
              next_arg += 1
            } else {
              yield {kind: "short", name: letter, value: "", has_value: false}
            }
          } else {
            yield {kind: "short", name: letter, value: "", has_value: false}
          }
        }
      }
    }
  }
}

pure suffix(format: Str) -> Str {
  match format { "gz" => ".gz", "bz2" => ".bz2", "xz" => ".xz", "zstd" => ".zst", _ => ".lzma" }
}

# The name a compressed file decodes to. A custom suffix (-S) is the only
# suffix recognized; `auto` lets xz also strip the lzma suffixes, which it
# decodes too until --format pins one container.
pure decoded_name(name: Str, format: Str, custom: Str?, auto: Bool) -> Str? {
  if custom != null {
    if name.ends_with(custom) and name.byte_len() > custom.byte_len() { return name.byte_slice(0, length: name.byte_len() - custom.byte_len()) }
    return null
  }
  let ending = suffix(format)
  if name.ends_with(ending) and name.byte_len() > ending.byte_len() {
    return name.byte_slice(0, length: name.byte_len() - ending.byte_len())
  }
  if format == "gz" and name.ends_with(".tgz") { return name.byte_slice(0, length: name.byte_len() - 4) + ".tar" }
  if format == "bz2" and name.ends_with(".tbz2") { return name.byte_slice(0, length: name.byte_len() - 5) + ".tar" }
  if format == "bz2" and name.ends_with(".tbz") { return name.byte_slice(0, length: name.byte_len() - 4) + ".tar" }
  if format == "bz2" and name.ends_with(".bz") { return name.byte_slice(0, length: name.byte_len() - 3) }
  if format == "xz" and name.ends_with(".txz") { return name.byte_slice(0, length: name.byte_len() - 4) + ".tar" }
  if format == "xz" and auto and name.ends_with(".lzma") and name.byte_len() > 5 { return name.byte_slice(0, length: name.byte_len() - 5) }
  if format == "xz" and auto and name.ends_with(".tlz") { return name.byte_slice(0, length: name.byte_len() - 4) + ".tar" }
  if format == "lzma" and name.ends_with(".tlz") { return name.byte_slice(0, length: name.byte_len() - 4) + ".tar" }
  if format == "zstd" and name.ends_with(".tzst") { return name.byte_slice(0, length: name.byte_len() - 5) + ".tar" }
  null
}

pure combined_status(current: Int, incoming: Int, format: Str) -> Int {
  if format == "bz2" {
    if incoming > current { incoming } else { current }
  } else if current == 1 or incoming == 1 {
    1
  } else {
    if incoming > current { incoming } else { current }
  }
}

# Percentage columns round to tenths of a percent; an empty payload has no
# ratio and gzip prints it as negative infinity.
pure gzip_ratio(uncompressed: Int, stored: Int) -> Str {
  if uncompressed == 0 { return " -Inf%" }
  let saved = uncompressed - stored
  let tenths = ((if saved < 0 { -saved } else { saved }) * 1000 + uncompressed / 2) / uncompressed
  let sign = if saved < 0 { "-" } else { "" }
  f"{f"{sign}{tenths / 10}.{tenths % 10}%":>6}"
}

pure has_bit(value: Int, bit: Int) -> Bool { (value / bit) % 2 == 1 }

# Length of a gzip member header: the fixed ten bytes plus whichever optional
# extra, name, comment and header-CRC fields its flag byte announces. Returns
# null when `head` is not a gzip header or is cut short.
pure gzip_header_length(head: Bytes) -> Int? {
  if head.len() < 10 or head.byte_at(0) != 31 or head.byte_at(1) != 139 or head.byte_at(2) != 8 { return null }
  let flags = head.byte_at(3) ?? 0
  var at = 10
  if has_bit(flags, 4) {
    if head.len() < at + 2 { return null }
    at += 2 + (head.byte_at(at) ?? 0) + 256 * (head.byte_at(at + 1) ?? 0)
  }
  for bit in [8, 16] {
    if has_bit(flags, bit) {
      while at < head.len() and head.byte_at(at) != 0 { at += 1 }
      at += 1
    }
  }
  if has_bit(flags, 2) { at += 2 }
  if at > head.len() { null } else { at }
}

# gzip -l: one row per archive from the header and trailer, with the
# uncompressed total taken from decoding so concatenated members add up. The
# ratio excludes the header and trailer of a lone member and nothing for a
# concatenation; the totals row reuses the last archive's overhead. Those are
# the figures gzip's own table shows.
proc list_gzip(names: List[Str], quiet: Bool, stored_names: Bool) [fs, io, error, process, env] -> Result[Int] {
  var status = 0
  var rows = 0
  var total_compressed = 0
  var total_uncompressed = 0
  var overhead = 0
  let scratch = fs.tempdir()?
  defer scratch.close()
  let work = scratch.host_path()?
  for name in names {
    var file = fp"{name}"
    var shown = if name == "-" { "stdout" } else { name }
    if name == "-" {
      scratch.write(p"stdin", io.stdin_bytes()?)
      file = fp"{work}/stdin"
    }
    guard let info = fs.stat(file) else { |failure|
      gnu.name_error(name, failure)
      status = 1
      continue
    }
    let head = bytes.read_at(file, 0, if info.size < 65536 { info.size } else { 65536 }, regular: true)?
    guard let header = gzip_header_length(head) else {
      gnu.error(f"{name}: not in gzip format")
      status = 1
      continue
    }
    let decoded = fp"{work}/decoded"
    if let Err(failure) = compression.transform(file, decoded, "gzip", decode: true, metadata: false, overwrite: true) {
      gnu.name_error(name, failure)
      status = 1
      continue
    }
    let uncompressed = fs.stat(decoded)?.size
    let trailer = bytes.read_at(file, info.size - 8, 8, regular: true)?
    let single = uncompressed == bytes.unpack_le(trailer, 4, 4)?
    overhead = if single { header + 8 } else { 0 }
    if name != "-" and name.byte_len() > 3 and name.ends_with(".gz") { shown = name.byte_slice(0, length: name.byte_len() - 3) }
    if stored_names {
      if let Ok(original) = compression.gzip_name(file) {
        if original != null { shown = if "/" in name { f"{fp"{name}".parent()}/{original}" } else { f"{original}" } }
      }
    }
    if rows == 0 and ! quiet {
      gnu.write_text("         compressed        uncompressed  ratio uncompressed_name\n")
    }
    rows += 1
    total_compressed += info.size
    total_uncompressed += uncompressed
    gnu.write_text(f"{info.size:>19} {uncompressed:>19} {gzip_ratio(uncompressed, info.size - overhead)} {shown}\n")
  }
  if rows > 1 and ! quiet {
    gnu.write_text(f"{total_compressed:>19} {total_uncompressed:>19} {gzip_ratio(total_uncompressed, total_compressed - overhead)} (totals)\n")
  }
  Ok(status)
}

# A little-endian base-128 integer as xz index records store sizes; `next` is
# the offset after it, or -1 when the data ends inside the number.
type VarInt = {value: Int, next: Int}

pure xz_varint(data: Bytes, at: Int) -> VarInt {
  var value = 0
  var scale = 1
  var cursor = at
  while cursor < data.len() and cursor < at + 9 {
    let byte = data.byte_at(cursor) ?? 0
    value += (byte % 128) * scale
    scale *= 128
    cursor += 1
    if byte < 128 { return {value: value, next: cursor} }
  }
  {value: 0, next: -1}
}

pure xz_check_name(id: Int) -> Str {
  match id {
    0 => "None"
    1 => "CRC32"
    4 => "CRC64"
    10 => "SHA-256"
    _ => f"Unknown-{id}"
  }
}

# Sizes print as bytes below 10000 and otherwise in the largest binary unit
# that keeps the value under 10000, with one decimal, as xz --list does.
pure xz_nice_size(value: Int) -> Str {
  if value < 10000 { return f"{value} B" }
  var divisor = 1024
  var unit = 0
  let units = ["KiB", "MiB", "GiB", "TiB"]
  while value >= 10000 * divisor and unit < 3 {
    divisor *= 1024
    unit += 1
  }
  let tenths = (value * 10 + divisor / 2) / divisor
  f"{tenths / 10}.{tenths % 10} {units[unit]}"
}

# The ratio column is compressed over uncompressed to three places; xz prints
# dashes for an empty payload and for ratios that no longer fit the column.
pure xz_ratio(compressed: Int, uncompressed: Int) -> Str {
  if uncompressed == 0 { return "---" }
  let thousandths = (compressed * 1000 + uncompressed / 2) / uncompressed
  if thousandths > 9999 { return "---" }
  f"{thousandths / 1000}.{f"{thousandths % 1000:03}"}"
}

type XzScan = {problem: Str, streams: Int, blocks: Int, uncompressed: Int, checks: List[Int]}

# Walk the streams of an .xz file from its end, reading each stream's index
# and flags, so no block data is decoded. `problem` is empty on success.
proc xz_scan(file: Path, size: Int) [fs, error] -> Result[XzScan] {
  var scan = {problem: "", streams: 0, blocks: 0, uncompressed: 0, checks: []}
  if size < 24 { return Ok({problem: "Too small to be a valid .xz file", streams: 0, blocks: 0, uncompressed: 0, checks: []}) }
  if bytes.read_at(file, 0, 6, regular: true)? != b"\xfd7zXZ\0" {
    return Ok({problem: "File format not recognized", streams: 0, blocks: 0, uncompressed: 0, checks: []})
  }
  let corrupt = {problem: "Compressed data is corrupt", streams: 0, blocks: 0, uncompressed: 0, checks: []}
  var end = size
  while end > 0 {
    while end >= 4 and bytes.read_at(file, end - 4, 4, regular: true)? == b"\0\0\0\0" { end -= 4 }
    if end < 24 { return Ok(corrupt) }
    let footer = bytes.read_at(file, end - 12, 12, regular: true)?
    if footer[10..12] != b"YZ" { return Ok(corrupt) }
    let index_size = (bytes.unpack_le(footer, 4, 4)? + 1) * 4
    if index_size + 24 > end { return Ok(corrupt) }
    let index = bytes.read_at(file, end - 12 - index_size, index_size, regular: true)?
    if index.byte_at(0) != 0 { return Ok(corrupt) }
    let counted = xz_varint(index, 1)
    if counted.next < 0 { return Ok(corrupt) }
    var cursor = counted.next
    var payload = 0
    var uncompressed = 0
    for _ in range(counted.value) {
      let unpadded = xz_varint(index, cursor)
      if unpadded.next < 0 { return Ok(corrupt) }
      let plain = xz_varint(index, unpadded.next)
      if plain.next < 0 { return Ok(corrupt) }
      cursor = plain.next
      payload += (unpadded.value + 3) / 4 * 4
      uncompressed += plain.value
    }
    let start = end - (12 + payload + index_size + 12)
    if start < 0 { return Ok(corrupt) }
    let flags = bytes.read_at(file, start + 6, 2, regular: true)?
    let check = (flags.byte_at(1) ?? 0) % 16
    var checks = scan.checks
    if check not in checks { checks += [check] }
    scan = {problem: "", streams: scan.streams + 1, blocks: scan.blocks + counted.value, uncompressed: scan.uncompressed + uncompressed, checks: checks}
    end = start
  }
  Ok(scan)
}

pure xz_check_label(checks: List[Int]) -> Str {
  collect { for id in range(0, 16) { if id in checks { yield xz_check_name(id) } } }.join(",")
}

# xz -l: a table with one row per archive and, for several, a totals row.
# The header appears with the first archive that can be read.
proc list_xz(names: List[Str]) [fs, io, error, process, env] -> Result[Int] {
  var status = 0
  var rows = 0
  var streams = 0
  var blocks = 0
  var compressed = 0
  var uncompressed = 0
  var checks: List[Int] = []
  for name in names {
    if name == "-" {
      gnu.error("--list does not support reading from standard input")
      return Ok(1)
    }
    let file = fp"{name}"
    guard let info = fs.stat(file) else { |failure|
      gnu.name_error(name, failure)
      status = 1
      continue
    }
    let scan = xz_scan(file, info.size)?
    if scan.problem != "" {
      gnu.error(f"{name}: {scan.problem}")
      status = 1
      continue
    }
    if rows == 0 { gnu.write_text("Strms  Blocks   Compressed Uncompressed  Ratio  Check   Filename\n") }
    rows += 1
    streams += scan.streams
    blocks += scan.blocks
    compressed += info.size
    uncompressed += scan.uncompressed
    for id in scan.checks { if id not in checks { checks += [id] } }
    gnu.write_text(f"{scan.streams:>5} {scan.blocks:>7} {xz_nice_size(info.size):>12} {xz_nice_size(scan.uncompressed):>12}  {xz_ratio(info.size, scan.uncompressed):>5}  {xz_check_label(scan.checks):<7} {name}\n")
  }
  if rows > 1 {
    gnu.write_text("-------------------------------------------------------------------------------\n")
    gnu.write_text(f"{streams:>5} {blocks:>7} {xz_nice_size(compressed):>12} {xz_nice_size(uncompressed):>12}  {xz_ratio(compressed, uncompressed):>5}  {xz_check_label(checks):<7} {rows} files\n")
  }
  Ok(status)
}

type ZstdScan = {problem: Str, frames: Int, skips: Int, uncompressed: Int, known: Bool, checksummed: Bool}

# Sizes print as bytes below 1 KiB and otherwise in the largest binary unit
# that keeps the value at least 1, with two decimals below 10, one below 100
# and none above (chosen before rounding), which is how zstd --list shows them.
pure zstd_nice_size(value: Int) -> Str {
  if value < 1024 { return f"{value:>6}   B" }
  var divisor = 1024
  var unit = 0
  let units = ["KiB", "MiB", "GiB", "TiB"]
  while value >= divisor * 1024 and unit < 3 {
    divisor *= 1024
    unit += 1
  }
  let shown = if value < 10 * divisor {
    let hundredths = (value * 100 + divisor / 2) / divisor
    f"{hundredths / 100}.{f"{hundredths % 100:02}"}"
  } else if value < 100 * divisor {
    let tenths = (value * 10 + divisor / 2) / divisor
    f"{tenths / 10}.{tenths % 10}"
  } else {
    f"{(value + divisor / 2) / divisor}"
  }
  f"{shown:>6} {units[unit]}"
}

# Frame by frame: skippable frames are stepped over by their declared length,
# zstd frames by walking block headers, so no block data is decoded. The
# content size is known only when every zstd frame declares it.
proc zstd_scan(file: Path, size: Int) [fs, error] -> Result[ZstdScan] {
  var frames = 0
  var skips = 0
  var uncompressed = 0
  var known = true
  var checksummed = false
  var at = 0
  let bad = {problem: "not a zstd frame sequence", frames: 0, skips: 0, uncompressed: 0, known: true, checksummed: false}
  while at < size {
    if size - at < 4 { return Ok(bad) }
    let magic = bytes.unpack_le(bytes.read_at(file, at, 4, regular: true)?, 4)?
    # Skippable frames use magics 0x184D2A50 through 0x184D2A5F; zstd frames
    # use 0xFD2FB528.
    if magic / 16 == 25481893 {
      if size - at < 8 { return Ok(bad) }
      let length = bytes.unpack_le(bytes.read_at(file, at + 4, 4, regular: true)?, 4)?
      at += 8 + length
      if at > size { return Ok(bad) }
      frames += 1
      skips += 1
      continue
    }
    if magic != 4247762216 { return Ok(bad) }
    let head = bytes.read_at(file, at + 4, if size - at - 4 < 14 { size - at - 4 } else { 14 }, regular: true)?
    if head.len() < 1 { return Ok(bad) }
    let descriptor = head.byte_at(0) ?? 0
    let size_flag = descriptor / 64
    let single = (descriptor / 32) % 2 == 1
    let has_checksum = (descriptor / 4) % 2 == 1
    let dict_flag = descriptor % 4
    let dict_bytes = match dict_flag { 0 => 0, 1 => 1, 2 => 2, _ => 4 }
    var cursor = 1 + (if single { 0 } else { 1 }) + dict_bytes
    let content_bytes = match size_flag { 0 => if single { 1 } else { 0 }, 1 => 2, 2 => 4, _ => 8 }
    if head.len() < cursor + content_bytes { return Ok(bad) }
    if content_bytes == 0 {
      known = false
    } else {
      uncompressed += bytes.unpack_le(head, content_bytes, cursor)? + (if size_flag == 1 { 256 } else { 0 })
    }
    at += 4 + cursor + content_bytes
    var last = false
    while ! last {
      if size - at < 3 { return Ok(bad) }
      let block = bytes.read_at(file, at, 3, regular: true)?
      let header = (block.byte_at(0) ?? 0) + 256 * (block.byte_at(1) ?? 0) + 65536 * (block.byte_at(2) ?? 0)
      last = header % 2 == 1
      let kind = (header / 2) % 4
      if kind == 3 { return Ok(bad) }
      at += 3 + (if kind == 1 { 1 } else { header / 8 })
      if at > size { return Ok(bad) }
    }
    if has_checksum {
      at += 4
      if at > size { return Ok(bad) }
      checksummed = true
    }
    frames += 1
  }
  Ok({problem: "", frames: frames, skips: skips, uncompressed: uncompressed, known: known, checksummed: checksummed})
}

pure zstd_ratio(uncompressed: Int, compressed: Int) -> Str {
  let thousandths = if compressed == 0 { 0 } else { (uncompressed * 1000 + compressed / 2) / compressed }
  f"{thousandths / 1000}.{f"{thousandths % 1000:03}"}"
}

pure zstd_row(frames: Int, skips: Int, compressed: Int, known: Bool, uncompressed: Int, check: Str, label: Str) -> Str {
  let plain = if known { zstd_nice_size(uncompressed) } else { "" }
  let ratio = if known { zstd_ratio(uncompressed, compressed) } else { "" }
  f"{frames:>6}  {skips:>5}  {zstd_nice_size(compressed):>10}  {plain:>12}  {ratio:>5}  {check:>5}  {label}\n"
}

# zstd -l: the header appears before any archive, errors name the file, and
# several archives end with a totals row after a dashed rule.
proc list_zstd(names: List[Str]) [fs, io, error, process, env] -> Result[Int] {
  if "-" in names {
    gnu.error("--list does not support reading from standard input")
    gnu.error("No files given")
    return Ok(1)
  }
  gnu.write_text("Frames  Skips  Compressed  Uncompressed  Ratio  Check  Filename\n")
  var status = 0
  var rows = 0
  var frames = 0
  var skips = 0
  var compressed = 0
  var uncompressed = 0
  var known = true
  var checks: List[Str] = []
  for name in names {
    let file = fp"{name}"
    guard let info = fs.stat(file) else { |failure|
      gnu.name_error(name, failure)
      status = 1
      continue
    }
    let scan = zstd_scan(file, info.size)?
    if scan.problem != "" {
      # zstd reports this on standard output, with its trailing space.
      gnu.write_text(f"File \"{name}\" not compressed by zstd \n")
      status = 1
      continue
    }
    rows += 1
    frames += scan.frames
    skips += scan.skips
    compressed += info.size
    uncompressed += scan.uncompressed
    if ! scan.known { known = false }
    let check = if scan.checksummed { "XXH64" } else { "None" }
    if check not in checks { checks += [check] }
    gnu.write_text(zstd_row(scan.frames, scan.skips, info.size, scan.known, scan.uncompressed, check, name))
  }
  if rows > 1 {
    gnu.write_text("----------------------------------------------------------------- \n")
    gnu.write_text(zstd_row(frames, skips, compressed, known, uncompressed, if checks.len() == 1 { checks[0] } else { "" }, f"{rows} files"))
  }
  Ok(status)
}

# xz option values are plain decimals; sizes may carry a binary multiplier. The
# numbers only bound what the codec is asked to do, so they are validated and
# otherwise left to the encoder's defaults.
proc xz_number(text: Str, option: Str, sizes: Bool) [error, process, env] -> Result[Int] {
  var end = 0
  while end < text.byte_len() and digit(text.byte_slice(end, length: 1)) { end += 1 }
  if end == 0 {
    gnu.error(f"{text}: Value is not a non-negative decimal integer")
    exit 1
  }
  let tail = text.byte_slice(end)
  if tail != "" {
    if ! sizes {
      gnu.error(f"{text}: Value is not a non-negative decimal integer")
      exit 1
    }
    if tail not in ["k", "kB", "K", "KB", "KiB", "M", "MB", "MiB", "G", "GB", "GiB"] {
      gnu.error(f"{tail}: Invalid multiplier suffix")
      gnu.error("Valid suffixes are 'KiB' (2^10), 'MiB' (2^20), and 'GiB' (2^30).")
      exit 1
    }
  }
  if end > 18 {
    gnu.error(f"Value of the option '{option}' must be in the range [0, {if sizes { "9223372036854775807" } else { "16384" }}]")
    exit 1
  }
  let value = text.byte_slice(0, length: end).parse_int()?
  if ! sizes and value > 16384 {
    gnu.error(f"Value of the option '{option}' must be in the range [0, 16384]")
    exit 1
  }
  Ok(value)
}

# Options the reference tool documents that this front end cannot honor. They
# fail by name instead of being skipped, because dropping them would change
# what the archive contains or which limits apply.
pure unsupported_option(format: Str, option: Str) -> Bool {
  if format == "zstd" {
    option in ["D", "b", "train", "train-cover", "train-fastcover", "train-legacy", "patch-from", "filelist", "output-dir-flat", "output-dir-mirror", "no-check", "trace", "exclude-compressed", "M", "memory", "maxdict", "dictID"]
  } else if format in ["xz", "lzma"] {
    option in ["M", "memlimit", "memory", "memlimit-compress", "memlimit-decompress", "memlimit-mt-decompress", "lzma1", "lzma2", "files", "files0", "block-list", "single-stream", "ignore-check", "robot", "no-adjust", "flush-timeout", "filters", "x86", "powerpc", "ia64", "arm", "armthumb", "arm64", "sparc", "riscv", "delta"]
  } else {
    false
  }
}

# Search tuning that never changes what the decoded stream contains. Each is
# accepted so scripts written for the reference tool keep working; the encoder
# here picks its own parameters, so the result is a valid archive that need not
# match the reference byte for byte.
pure tuning_option(format: Str, option: Str) -> Bool {
  if format == "zstd" {
    option in ["progress", "no-progress", "adapt", "rsyncable", "single-thread", "auto-threads", "check", "compress-literals", "no-compress-literals", "row-match-finder", "no-row-match-finder", "asyncio", "no-asyncio", "mmap-dict", "no-mmap-dict", "sparse", "no-sparse", "stream-size", "size-hint", "target-compressed-block-size", "B", "no-dictID"]
  } else if format in ["xz", "lzma"] {
    option in ["e", "extreme", "no-sparse"]
  } else if format == "gz" {
    option in ["rsyncable", "synchronous"]
  } else {
    option == "exponential"
  }
}

type Expansion = {names: List[Str], status: Int}

# -r replaces each directory operand with the regular files below it, in name
# order, spelled relative to the operand as typed. gzip leaves out files that
# are not its kind of input (already suffixed when compressing, unsuffixed when
# decoding) unless forced; it says so, and counts it as a warning, only when it
# is verbose or testing, which is how it can be pointed at a mixed tree. zstd
# tries every file.
proc expand_directories(names: List[Str], format: Str, decode: Bool, custom: Str?, auto: Bool, force: Bool, explain: Bool) [fs, process, env, error] -> Result[Expansion] {
  var expanded: List[Str] = []
  var status = 0
  for name in names {
    if name == "-" or ! (fp"{name}".is_dir() ?? false) {
      expanded += [name]
      continue
    }
    let prefix = if name.ends_with("/") { name } else { f"{name}/" }
    var top = ""
    for entry in fs.walk(fp"{name}", hidden: true)? |> sort-by .path {
      let text = entry.path.display()
      # Entries come back absolute and sorted, so the directory itself is first
      # and every other path extends it.
      if top == "" { top = text }
      if entry.kind == "dir" { continue }
      let found = f"{prefix}{text.byte_slice(top.byte_len() + 1)}"
      if format == "gz" and ! force {
        if decode and decoded_name(found, format, custom, auto) == null {
          if explain {
            gnu.error(f"{found}: unknown suffix -- ignored")
            status = 2
          }
          continue
        }
        if ! decode and found.ends_with(custom ?? suffix(format)) {
          if explain { gnu.error(f"{found} already has {custom ?? suffix(format)} suffix -- unchanged") }
          continue
        }
      }
      expanded += [found]
    }
  }
  Ok({names: expanded, status: status})
}

# zstd -o with several inputs writes one file holding every input's frames in
# order. Frames concatenate, so each input is encoded on its own and the results
# are joined. The reference tool asks before doing this, which a script cannot
# answer, so -f stands in for the confirmation. Returns the exit status.
proc write_concatenated(names: List[Str], output: Str, codec: Str, decode: Bool, level: Int, force: Bool, keep: Bool, show_errors: Bool, show_warnings: Bool) [fs, io, error, process, env] -> Int {
  if show_warnings {
    gnu.error(f"WARNING: all input files will be processed and concatenated into a single output file: {output}")
    gnu.error("The concatenated output CANNOT regenerate original file names nor directory structure.")
  }
  if ! force {
    gnu.error("Proceed? (y/n): Aborting...")
    return 1
  }
  let scratch = fs.tempdir()?
  defer scratch.close()
  let part = fp"{scratch.host_path()?}/part"
  var joined: List[Bytes] = []
  var consumed: List[Str] = []
  var status = 0
  for name in names {
    if name == output {
      if show_errors { gnu.error("Refusing to open an output file which will overwrite the input file") }
      status = 1
      continue
    }
    var source: Path? = null
    if name != "-" {
      guard let info = fs.stat(fp"{name}") else { |failure|
        if show_errors { gnu.error(f"can't stat {name} : {gnu.strerror(failure)} -- ignored") }
        status = 1
        continue
      }
      if info.kind != "file" {
        if show_errors { gnu.error(f"{name} is not a regular file -- ignored") }
        status = 1
        continue
      }
      source = fp"{name}"
    }
    match compression.transform(source, part, codec, decode: decode, level: level, test: false, metadata: false, overwrite: true, pass_through: false) {
      Ok(_) => {
        joined += [part.read_bytes()?]
        if name != "-" { consumed += [name] }
      }
      Err(failure) => {
        if show_errors { gnu.name_error(name, failure) }
        status = 1
      }
    }
  }
  if joined.is_empty() { return status }
  fp"{output}".write_atomic(bytes.concat(joined))?
  if ! keep {
    for name in consumed {
      if let Err(failure) = fp"{name}".remove(missing_ok: false) {
        if show_errors { gnu.name_error(name, failure) }
        status = 1
      }
    }
  }
  status
}

## Run one compression applet with the supplied codec and alias defaults.
export proc execute(argv: List[Str], tool: Str, decoding: Bool, cat: Bool) [fs, io, error, process, env] {
  # --format can move an xz or lzma applet to the other container, so the
  # family below is a variable; `tool` only picks the option grammar.
  var format = tool
  var auto_format = tool == "xz"
  var decode = decoding
  var stdout = cat
  var keep = format == "zstd"
  var force = false
  var pass_through = false
  var testing = false
  var quiet = 0
  var no_warn = false
  var verbose = false
  var listing = false
  var recursive = false
  var stored_names = false
  var output_file: Str? = null
  var custom_suffix: Str? = null
  var name_mode: Bool? = null
  var level = if format == "bz2" { 9 } else if format == "zstd" { 3 } else { 6 }
  var ultra = false
  var names = []
  for token in option_tokens(argv, tool) {
    if token.kind == "operand" { names += [token.name]; continue }
    let option = token.name
    let shown = f"{if token.kind == "short" { "-" } else { "--" }}{option}"
    if unsupported_option(format, option) {
      gnu.error(f"option '{shown}' is not supported")
      exit 1
    }
    let needs_value = if token.kind == "short" { short_takes_value(format, option) } else { long_takes_value(format, option) }
    if needs_value and ! token.has_value {
      if format == "zstd" and option == "o" {
        gnu.error("error: missing command argument")
        exit 1
      }
      gnu.usage_error(f"option '{shown}' requires an argument")
    }
    let optional_value = format == "zstd" and option in ["fast", "long", "format", "auto-threads", "stream-size", "size-hint", "target-compressed-block-size"]
    if token.kind == "long" and token.has_value and ! (needs_value or optional_value) { gnu.usage_error(f"option '--{option}' does not allow an argument") }
    if option in ["c", "stdout", "to-stdout"] {
      stdout = true
      output_file = null
    } else if option in ["d", "decompress", "uncompress"] {
      decode = true
    } else if option in ["z", "compress"] {
      decode = false
    } else if option in ["k", "keep"] {
      keep = true
    } else if option == "rm" and format == "zstd" {
      keep = false
    } else if option in ["f", "force"] {
      force = true
    } else if option in ["t", "test"] {
      testing = true
      decode = true
    } else if option in ["n", "no-name"] and format == "gz" {
      name_mode = false
    } else if option in ["N", "name"] and format == "gz" {
      name_mode = true
      stored_names = true
    } else if option in ["l", "list"] and format in ["gz", "xz", "zstd"] {
      listing = true
    } else if option in ["s", "small"] and format == "bz2" {
      # bzip2 -s trades speed for memory; the codec here has no such mode.
      continue
    } else if option in ["q", "quiet"] {
      quiet += 1
    } else if option in ["v", "verbose"] {
      verbose = true
    } else if option == "o" and format == "zstd" {
      # A following option or "-" is not a file name: the reference tool
      # reports the pair as separated by another command.
      if token.value.starts_with("-") {
        gnu.error("error: command cannot be separated from its argument by another command")
        exit 1
      }
      output_file = token.value
      stdout = false
    } else if (option == "r" and format in ["gz", "zstd"]) or (option == "recursive" and format == "gz") {
      recursive = true
    } else if option in ["T", "threads"] and format in ["xz", "lzma"] {
      # Threads only split the work; the stream decodes the same.
      let _ = xz_number(token.value, "threads", false)?
    } else if option in ["T", "threads"] and format == "zstd" {
      if token.kind == "long" and ! rx"^[0-9]+(K|KB|KiB|M|MB|MiB)?$".matches(token.value) {
        gnu.error("error: only numeric values with optional suffixes K, KB, KiB, M, MB, MiB are allowed")
        exit 1
      }
    } else if option == "block-size" and format in ["xz", "lzma"] {
      let _ = xz_number(token.value, "block-size", true)?
    } else if option in ["Q", "no-warn"] and format in ["xz", "lzma"] {
      no_warn = true
    } else if option in ["F", "format"] and format in ["xz", "lzma"] {
      if token.value == "auto" {
        format = "xz"
        auto_format = true
      } else if token.value == "xz" {
        format = "xz"
        auto_format = false
      } else if token.value in ["lzma", "alone"] {
        format = "lzma"
        auto_format = false
      } else if token.value == "raw" {
        gnu.error("option '--format=raw' is not supported")
        exit 1
      } else {
        gnu.error(f"{token.value}: Unknown file format type")
        exit 1
      }
    } else if option in ["C", "check"] and format in ["xz", "lzma"] {
      # The codec always writes CRC64; naming it is accepted, any other check
      # would silently produce a different archive.
      if token.value in ["crc32", "sha256", "none"] {
        gnu.error(f"option '--check={token.value}' is not supported")
        exit 1
      } else if token.value != "crc64" {
        gnu.error(f"{token.value}: Unsupported integrity check type")
        exit 1
      }
    } else if option in ["S", "suffix"] and format in ["gz", "xz", "lzma"] {
      if token.value == "" or token.value.byte_len() > 30 {
        gnu.error(f"'{token.value}': invalid suffix")
        exit 1
      }
      custom_suffix = token.value
    } else if option == "format" and format == "zstd" {
      if token.value != "zstd" {
        gnu.error(f"Incorrect parameter: --format={token.value}")
        exit 1
      }
    } else if option == "long" and format == "zstd" {
      # The window log is a compression hint, so a valid value changes nothing
      # the decoder sees; the reference tool also lets a non-number through.
      if rx"^[0-9]+$".matches(token.value) {
        let window = token.value.parse_int()?
        if window < 10 or window > 31 {
          gnu.error("error 11 : Parameter is out of bound")
          exit 11
        }
      }
    } else if option == "pass-through" and format == "zstd" {
      pass_through = true
    } else if option == "no-pass-through" and format == "zstd" {
      pass_through = false
    } else if option == "fast" {
      if format == "zstd" {
        let speed = if token.value == "" { 1 } else { token.value.parse_int()? }
        if speed < 1 { gnu.usage_error("fast level must be at least 1") }
        level = -(if speed > 131072 { 131072 } else { speed })
      } else { level = 1 }
    } else if option == "ultra" and format == "zstd" {
      ultra = true
    } else if option == "best" {
      level = 9
    } else if tuning_option(format, option) {
      continue
    } else if token.kind == "short" and rx"^[0-9]+$".matches(option) {
      level = option.parse_int()?
      if (format != "zstd" and level > 9) or (format in ["gz", "bz2"] and level == 0) { gnu.usage_error("invalid compression level") }
    } else if option in ["repetitive-fast", "repetitive-best"] and format == "bz2" {
      if quiet == 0 { gnu.error(f"--{option} is redundant in versions 0.9.5 and above") }
    } else if option in ["h", "help", "H", "long-help"] {
      gnu.help(f"Usage: {gnu.prog()} [OPTION]... [FILE]...\nCompress or decompress files; no FILE or '-' reads standard input.\n  -c, --stdout      write to standard output\n  -d, --decompress  decompress\n  -k, --keep        keep input files\n  -f, --force       overwrite output files\n  -t, --test        check compressed data integrity\n  -1 .. -9          compression level\n")
      return
    } else if option in ["V", "version"] or (option in ["L", "license"] and format in ["gz", "bz2"]) {
      gnu.version(gnu.prog())
      return
    } else {
      gnu.usage_error(f"unrecognized option '{shown}'")
    }
  }
  # -q hides warnings; errors survive one -q in xz and zstd, and gzip and
  # bzip2 never silence them.
  let show_warnings = quiet == 0
  let show_errors = format in ["gz", "bz2"] or quiet < 2
  # What a skipped operand does to the exit status: xz and gzip call it a
  # warning (status 2, which xz -Q turns into 0); bzip2 and zstd call it an error.
  let skip_status = if format in ["bz2", "zstd"] { 1 } else if no_warn { 0 } else { 2 }
  if format == "zstd" {
    let ceiling = if ultra { 22 } else { 19 }
    if level > ceiling {
      if show_warnings { gnu.error(f"Warning : compression level higher than max, reduced to {ceiling}") }
      level = ceiling
    }
  }
  let metadata = name_mode ?? ! decode
  if names.is_empty() { names = ["-"] }
  var status = 0
  if recursive {
    let expansion = expand_directories(names, format, decode or listing, custom_suffix, auto_format, force, (verbose or testing) and show_warnings)?
    names = expansion.names
    status = expansion.status
  }
  if listing and verbose { gnu.usage_error("--verbose cannot be combined with --list") }
  if listing {
    match if format == "xz" { list_xz(names) } else if format == "zstd" { list_zstd(names) } else { list_gzip(names, quiet > 0, stored_names) } {
      Ok(failed) => { exit failed }
      Err(failure) => { gnu.error(gnu.strerror(failure)); exit 1 }
    }
  }
  # xz also reads .lzma content, so a decoding xz applet lets the codec pick
  # between the two from the stream itself; every other applet names its format.
  let codec = if decode and format == "xz" and auto_format { "xz,lzma" } else { format }
  var fixed: Path? = null
  if let target = output_file {
    if names.len() > 1 and ! testing {
      exit write_concatenated(names, target, codec, decode, level, force, keep, show_errors, show_warnings)
    }
    fixed = fp"{target}"
  }
  let active_suffix = custom_suffix ?? suffix(format)
  for name in names {
    let to_stdout = fixed == null and (stdout or name == "-")
    let source: Path? = if name == "-" { null } else { fp"{name}" }
    var destination: Path? = if testing { null } else { fixed }
    if ! to_stdout and ! testing and name != "-" {
      guard let info = fs.stat(fp"{name}", follow_symlinks: force) else { |failure|
        if show_errors { gnu.name_error(name, failure) }
        status = combined_status(status, 1, format)
        continue
      }
      # zstd compresses hard-linked files, and says so when it skips one.
      if info.kind != "file" or (! force and info.nlink > 1 and format != "zstd") {
        if format == "zstd" {
          if show_errors { gnu.error(f"{name} {if info.kind == "dir" { "is a directory" } else { "is not a regular file" }} -- ignored") }
        } else if show_warnings {
          gnu.error(f"{name}: not a regular file with one link -- skipped")
        }
        status = combined_status(status, skip_status, format)
        continue
      }
      if output_file == name {
        if show_errors { gnu.error("Refusing to open an output file which will overwrite the input file") }
        status = combined_status(status, 1, format)
        continue
      }
      if fixed == null {
        if decode {
          let output = decoded_name(name, format, custom_suffix, auto_format)
          if output == null {
            # gzip calls an unknown suffix a warning; the others fail the file.
            if format == "gz" {
              if show_warnings { gnu.error(f"{name}: unknown suffix - ignored") }
              status = combined_status(status, 1, format)
            } else {
              if show_errors { gnu.error(f"{name}: unknown suffix - ignored") }
              status = combined_status(status, 1, format)
            }
            continue
          }
          destination = fp"{output}"
          if format == "gz" and metadata {
            match compression.gzip_name(fp"{name}") {
              Ok(original) => { if original != null { destination = fp"{fp"{name}".parent()}/{original}" } }
              Err(failure) => { if show_errors { gnu.name_error(name, failure) }; status = combined_status(status, 1, format); continue }
            }
          }
        } else {
          # zstd compresses an already-suffixed name again, as the reference does.
          if format != "zstd" and name.ends_with(active_suffix) and (format != "gz" or ! force) {
            if show_warnings { gnu.error(f"{name} already has {active_suffix} suffix -- unchanged") }
            status = combined_status(status, skip_status, format)
            continue
          }
          destination = fp"{name}{active_suffix}"
        }
      }
    }
    if source == null and ! testing and ! decode and unix.isatty(0) and ! force {
      if show_errors { gnu.error("compressed data not read from a terminal; use -f to force") }
      status = combined_status(status, 1, format)
      continue
    }
    if to_stdout and ! testing and ! decode and unix.isatty(1) and ! force {
      if show_errors { gnu.error("compressed data not written to a terminal; use -f to force") }
      status = combined_status(status, 1, format)
      continue
    }
    let passing_through = (force or pass_through) and decode and to_stdout and ! testing
    match compression.transform(source, destination, codec, decode: decode, level: level, test: testing, metadata: metadata, overwrite: force, pass_through: passing_through) {
      Ok(_) => {
        if destination != null and source != null and ! keep {
          if let Err(failure) = fp"{name}".remove(missing_ok: false) {
            if show_errors { gnu.name_error(name, failure) }
            status = combined_status(status, 1, format)
          }
        }
        if verbose and show_warnings { gnu.error(f"{name}: {if testing { "OK" } else { "done" }}") }
      }
      Err(failure) => {
        if show_errors {
          # A codec failure has no OS error number; these BusyBox-worded
          # diagnostics replace the codec's own text for bzip2 and lzma input.
          if decode and gnu.errno(failure) == 0 and format in ["bz2", "lzma"] {
            gnu.error(if format == "bz2" { "bunzip error -5" } else { "corrupted data" })
          } else if destination != null and gnu.errno(failure) == 17 {
            # EEXIST from the no-clobber publish step: the output name already exists.
            gnu.error(f"can't open {gnu.quote(f"{destination}")}: {gnu.strerror(failure)}")
          } else {
            gnu.name_error(name, failure)
          }
        }
        status = combined_status(status, 1, format)
      }
    }
  }
  exit status
}
