#!/bin/xsh
use lib.gnu

const USAGE = """Usage: od [OPTION]... [FILE]...
Write an unambiguous representation of FILE to standard output.

  -A RADIX           output offsets in radix d, o, x, or n
      --traditional   accept the traditional FILE OFFSET LABEL syntax
  -j BYTES           skip BYTES input bytes
  -N BYTES           limit output to BYTES input bytes
  -t TYPE            select output format
  -v                 do not suppress repeated output lines
  -w[BYTES]          output BYTES bytes per output line (default 16)
      --endian=TYPE  select little or big endian
      --help         display this help and exit
      --version      output version information and exit
"""

type Options = {
  address: Str, skip: Str, read: Str, formats: List[Str], width: Str, endian: Str,
  duplicates: Bool, strings: Str, traditional: Bool, help: Bool, version: Bool,
  short_a: Bool, short_b: Bool, short_c: Bool, short_d: Bool, short_o: Bool, short_s: Bool,
  short_x: Bool, short_X: Bool, short_f: Bool, short_i: Bool, short_l: Bool,
  upper_dec: Bool, upper_float: Bool, upper_hex: Bool, upper_int: Bool, upper_long: Bool, upper_oct: Bool,
  files: List[Str]
}
type Format = {base: Str, size: Int, mode: Str, ascii: Bool}
type OffsetParts = {digits: Str, radix: Int, multiplier: Int}

pure raw_for(argv: List[Str], raw: List[Bytes], name: Str) -> Bytes {
  for index in range(argv.len()) { if argv[index] == name { return raw[index] } }
  bytes.from_text(name)
}

pure parse_nonnegative(text: Str) -> Int? {
  if text.starts_with("0x") or text.starts_with("0X") { return parse_radix(text.byte_slice(2), 16, "0123456789abcdef") }
  if text.starts_with("0") and text.byte_len() > 1 { return parse_radix(text.byte_slice(1), 8, "01234567") }
  parse_radix(text, 10, "0123456789")
}

pure parse_radix(text: Str, radix: Int, alphabet: Str) -> Int? {
  if text == "" { return null }
  var value = 0
  for index in range(text.byte_len()) {
    let digit = alphabet.find(text.byte_slice(index, 1).lower())
    if digit == null or (digit ?? 0) >= radix { return null }
    if value > (9223372036854775807 - (digit ?? 0)) / radix { return null }
    value = value * radix + (digit ?? 0)
  }
  value
}

pure pad_left(text: Str, width: Int) -> Str {
  if text.byte_len() >= width { return text }
  tui.left_pad(text, width)
}

pure pad_right(text: Str, width: Int) -> Str {
  if text.byte_len() >= width { return text }
  tui.right_pad(text, width)
}

pure display_width(format: Format) -> Int {
  if format.base == "x" { return format.size * 2 + (if format.ascii { 1 } else { 0 }) }
  if format.mode == "char" or format.mode == "ascii" { return 3 }
  if format.base == "o" { return if format.size == 4 { 11 } else if format.size == 8 { 22 } else { format.size * 3 } }
  if format.mode == "signed" { return if format.size == 4 { 11 } else if format.size == 8 { 20 } else { format.size * 3 } }
  if format.size == 4 { return 10 }
  if format.size == 8 { return 20 }
  if format.size == 2 { return 5 }
  3
}

pure pad_zero(text: Str, width: Int) -> Str {
  if text.byte_len() >= width { return text }
  var out = ""
  for _ in range(width - text.byte_len()) { out = f"{out}0" }
  f"{out}{text}"
}

pure radix_text(value: Int, radix: Str) -> Str {
  if radix == "d" { return f"{value}" }
  let base = if radix == "x" { 16 } else { 8 }
  let alphabet = if radix == "x" { "0123456789abcdef" } else { "01234567" }
  if value == 0 { return "0" }
  let negative = value < 0
  var amount = if negative { -value } else { value }
  var out = ""
  while amount > 0 {
    out = f"{digit_char(amount % base, alphabet)}{out}"
    amount = amount / base
  }
  if negative { f"-{out}" } else { out }
}

pure offset_text(value: Int, radix: Str) -> Str {
  if radix == "n" { return "" }
  pad_zero(radix_text(value, radix), if radix == "x" { 6 } else { 7 })
}

pure formatted_address(value: Int, radix: Str, label: Int?) -> Str {
  if label == null { return offset_text(value, radix) }
  let shown_label = f"({offset_text(label ?? 0, "o")})"
  if radix == "n" { shown_label } else { f"{offset_text(value, radix)} {shown_label}" }
}

pure uses_modern_option(argv: List[Str]) -> Bool {
  for arg in argv {
    if arg == "--" { return false }
    if arg in ["-A", "-j", "-N", "-t", "-v", "-w", "-S", "--address-radix", "--skip-bytes", "--read-bytes", "--format", "--output-duplicates", "--width", "--strings"] { return true }
    if arg.starts_with("--address-radix=") or arg.starts_with("--skip-bytes=") or arg.starts_with("--read-bytes=") or arg.starts_with("--format=") or arg.starts_with("--width=") or arg.starts_with("--strings=") { return true }
    if arg.starts_with("-") and ! arg.starts_with("--") and arg != "-" {
      for index in range(1, arg.byte_len()) {
        if arg.byte_slice(index, 1) in ["A", "j", "N", "t", "v", "w", "S"] { return true }
      }
    }
  }
  false
}

pure digit_char(value: Int, alphabet: Str) -> Str { alphabet.byte_slice(value, 1) }

pure unsigned_value(data: Bytes, start: Int, size: Int, little: Bool) -> Int {
  var value = 0
  let count = if size > data.len() - start { data.len() - start } else { size }
  for at in range(count) {
    let shift = if little { at } else { count - at - 1 }
    if shift < 7 { value += (data.byte_at(start + at) ?? 0) * [1, 256, 65536, 16777216, 4294967296, 1099511627776, 281474976710656][shift] }
  }
  value
}

pure hex_word(data: Bytes, start: Int, size: Int, little: Bool) -> Str {
  var out = ""
  for index in range(size) {
    let source = if little { start + size - index - 1 } else { start + index }
    let byte = if source < data.len() { data.byte_at(source) ?? 0 } else { 0 }
    out = f"{out}{digit_char(byte / 16, "0123456789abcdef")}{digit_char(byte % 16, "0123456789abcdef")}"
  }
  out
}

pure numeric_word(data: Bytes, start: Int, format: Format, little: Bool) -> Str {
  let unsigned = unsigned_value(data, start, format.size, little)
  if format.base == "x" { return pad_left(hex_word(data, start, format.size, little), format.size * 2 + (if format.ascii { 1 } else { 0 })) }
  let width = if format.base == "o" { if format.size == 4 { 11 } else if format.size == 8 { 22 } else { format.size * 3 } } else if format.mode == "signed" { if format.size == 4 { 11 } else if format.size == 8 { 20 } else { format.size * 3 } } else if format.size == 4 { 10 } else if format.size == 8 { 20 } else if format.size == 2 { 5 } else { 3 }
  if format.base == "o" { return pad_zero(radix_text(unsigned, "o"), width) }
  if format.mode == "signed" and format.size < 8 {
    let bits = format.size * 8
    let sign = [128, 32768, 8388608, 2147483648][format.size - 1]
    if unsigned >= sign { return pad_left(f"{unsigned - sign * 2}", width) }
  }
  pad_left(f"{unsigned}", width)
}

pure char_word(data: Bytes, start: Int, ascii: Bool) -> Str {
  let byte = data.byte_at(start) ?? 0
  let names = ["nul", "soh", "stx", "etx", "eot", "enq", "ack", "bel", "bs", "ht", "nl", "vt", "ff", "cr", "so", "si", "dle", "dc1", "dc2", "dc3", "dc4", "nak", "syn", "etb", "can", "em", "sub", "esc", "fs", "gs", "rs", "us"]
  if byte < 32 {
    let shown = if ascii { names[byte] } else if byte == 0 { "\\0" } else if byte == 7 { "\\a" } else if byte == 8 { "\\b" } else if byte == 9 { "\\t" } else if byte == 10 { "\\n" } else if byte == 11 { "\\v" } else if byte == 12 { "\\f" } else if byte == 13 { "\\r" } else { pad_zero(radix_text(byte, "o"), 3) }
    return pad_left(shown, 3)
  }
  if ascii {
    let low_byte = byte % 128
    if low_byte == 32 { return pad_left("sp", 3) }
    if low_byte == 127 { return "del" }
    if low_byte < 32 { return names[low_byte] }
    let text = bytes.from_ints([low_byte]) ?? b""
    return pad_left(text.utf8() ?? "", 3)
  }
  if byte == 127 { return "177" }
  if byte >= 32 and byte <= 126 { return pad_left(data[start..start + 1].utf8() ?? "", 3) }
  pad_left(pad_zero(radix_text(byte, "o"), 3), 3)
}

pure strings_output(data: Bytes, start_offset: Int, minimum: Int, show_final: Bool, address: Str, label: Int?) -> Str {
  var out = ""
  var current = ""
  var current_start = 0
  for index in range(data.len()) {
    let byte = data.byte_at(index) ?? 0
    if byte >= 32 and byte <= 126 {
      if current == "" { current_start = index }
      current = f"{current}{data[index..index + 1].utf8() ?? ""}"
    } else {
      if current.byte_len() >= minimum {
        let shown = formatted_address(start_offset + current_start, address, if label == null { null } else { (label ?? 0) + current_start })
        let head = if shown == "" { "" } else { f"{shown} " }
        out = f"{out}{head}{current}\n"
      }
      current = ""
    }
  }
  if show_final and current.byte_len() >= minimum {
    let shown = formatted_address(start_offset + current_start, address, if label == null { null } else { (label ?? 0) + current_start })
    let head = if shown == "" { "" } else { f"{shown} " }
    out = f"{out}{head}{current}\n"
  }
  out
}

proc read_file(name: Str, raw_name: Bytes) [fs, process, env, error, io] -> Bytes {
  if name == "-" {
    match io.stdin_bytes() { Ok(data) => data, Err(failure) => { gnu.error(f"read error: {gnu.strerror(failure)}"); exit 1; b"" } }
  } else {
    let file_path = Path.parse_bytes(raw_name)?
    let is_directory = match fs.stat(file_path, follow_symlinks: true) { Ok(meta) => meta.kind == "dir", Err(_) => false }
    if is_directory { gnu.error(f"{gnu.quote_bytes(raw_name, always: false)}: Is a directory"); exit 1 }
    match file_path.read_bytes() {
      Ok(data) => data,
      Err(failure) => { gnu.error(f"{gnu.quote_bytes(raw_name, always: false)}: {gnu.strerror(failure)}"); exit 1; b"" },
    }
  }
}

pure offset_parts(text: Str) -> OffsetParts? {
  var value = text
  if value.starts_with("+") { value = value.byte_slice(1) }
  var multiplier = 1
  if value.ends_with("b") or value.ends_with("B") { multiplier = 512; value = value.byte_slice(0, value.byte_len() - 1) }
  var radix = 8
  if value.ends_with(".") { radix = 10; value = value.byte_slice(0, value.byte_len() - 1) }
  if value.starts_with("0x") or value.starts_with("0X") { radix = 16; value = value.byte_slice(2) }
  if value == "" { return null }
  {digits: value, radix: radix, multiplier: multiplier}
}

pure offset_digits_valid(text: Str, radix: Int) -> Bool {
  if text == "" { return false }
  let alphabet = "0123456789abcdef"
  for index in range(text.byte_len()) {
    let digit = alphabet.find(text.byte_slice(index, 1).lower())
    if digit == null or (digit ?? 0) >= radix { return false }
  }
  true
}

pure parse_traditional_offset(text: Str) -> Int? {
  let parts = offset_parts(text)
  if parts == null { return null }
  let actual = parts ?? {digits: "", radix: 8, multiplier: 1}
  let value = parse_radix(actual.digits, actual.radix, "0123456789abcdef")
  if value == null or (value ?? 0) > 9223372036854775807 / actual.multiplier { return null }
  (value ?? 0) * actual.multiplier
}

pure traditional_offset_overflows(text: Str) -> Bool {
  let parts = offset_parts(text)
  if parts == null { return false }
  let actual = parts ?? {digits: "", radix: 8, multiplier: 1}
  return false when ! offset_digits_valid(actual.digits, actual.radix)
  let value = parse_radix(actual.digits, actual.radix, "0123456789abcdef")
  value == null or (value ?? 0) > 9223372036854775807 / actual.multiplier
}

pure traditional_offset_candidate(text: Str) -> Bool {
  let parts = offset_parts(text)
  parts != null and offset_digits_valid((parts ?? {digits: "", radix: 8, multiplier: 1}).digits, (parts ?? {digits: "", radix: 8, multiplier: 1}).radix)
}

proc check_traditional_offset(text: Str) [process, env] -> Int {
  if traditional_offset_overflows(text) {
    gnu.error(f"{text}: Numerical result out of range")
    exit 1
  }
  parse_traditional_offset(text) ?? 0
}

pure parse_type(text: Str) -> Format? {
  let ascii = text.ends_with("z")
  let spec = if ascii { text.byte_slice(0, text.byte_len() - 1) } else { text }
  if spec == "c" { return {base: "c", size: 1, mode: "char", ascii: ascii} }
  if spec == "a" { return {base: "a", size: 1, mode: "ascii", ascii: ascii} }
  if spec.byte_len() < 2 { return null }
  let base = spec.byte_slice(0, 1)
  let suffix = spec.byte_slice(1)
  let size = parse_nonnegative(suffix) ?? (if suffix == "" { 0 } else { -1 })
  if base not in ["x", "o", "d", "u"] { return null }
  let actual = if size == 0 and base == "f" { 8 } else if size == 0 { 2 } else { size }
  if actual not in [1, 2, 4, 8] { return null }
  {base: if base == "u" { "d" } else { base }, size: actual, mode: if base == "d" { "signed" } else { "unsigned" }, ascii: ascii}
}

pure split_format_types(text: Str) -> List[Str] {
  var out: List[Str] = []
  var at = 0
  while at < text.byte_len() {
    let kind = text.byte_slice(at, 1)
    var end = at + 1
    if kind in ["x", "o", "d", "u", "f"] {
      while end < text.byte_len() and text.byte_slice(end, 1) in ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"] { end += 1 }
    }
    if end < text.byte_len() and text.byte_slice(end, 1) == "z" { end += 1 }
    out += [text.byte_slice(at, end - at)]
    at = end
  }
  out
}

pure short_format(arg: Str) -> Str? {
  if arg == "-a" { return "a" }
  if arg == "-b" or arg == "-B" { return "o1" }
  if arg == "-c" or arg == "-C" { return "c" }
  if arg == "-d" { return "u2" }
  if arg == "-D" { return "u4" }
  if arg == "-f" { return "f8" }
  if arg == "-F" { return "f8" }
  if arg == "-i" { return "d4" }
  if arg == "-I" { return "d4" }
  if arg == "-l" { return "d4" }
  if arg == "-L" { return "d8" }
  if arg == "-o" { return "o2" }
  if arg == "-O" { return "o4" }
  if arg == "-s" { return "d2" }
  if arg == "-x" { return "x2" }
  if arg == "-H" or arg == "-X" { return "x4" }
  null
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: Options = cli.applet(
    argv,
    {
      gnu: {status: 1},
      address: {form: "-A --address-radix RADIX", default: "o"},
      skip: {form: "-j --skip-bytes BYTES", default: "0"},
      read: {form: "-N --read-bytes BYTES", default: ""},
      formats: {form: "-t --format TYPE", repeated: true},
      width: {form: "-w --width[=BYTES]", default: "16", optional_default: "32", optional_value: true},
      endian: {form: "--endian TYPE", default: "little"},
      duplicates: {form: "-v --output-duplicates", default: false},
      strings: {form: "-S --strings[=BYTES]", default: "", optional_default: "3", optional_value: true},
      traditional: {form: "--traditional", default: false},
      short_a: {form: "-a", default: false}, short_b: {form: "-b", default: false}, short_c: {form: "-c", default: false},
      short_d: {form: "-d", default: false}, short_o: {form: "-o", default: false},
      short_s: {form: "-s", default: false}, short_x: {form: "-x", default: false},
      short_X: {form: "-X", default: false}, short_f: {form: "-f", default: false},
      short_i: {form: "-i", default: false}, short_l: {form: "-l", default: false},
      upper_dec: {form: "-D", default: false}, upper_float: {form: "-F", default: false}, upper_hex: {form: "-H", default: false},
      upper_int: {form: "-I", default: false}, upper_long: {form: "-L", default: false}, upper_oct: {form: "-O", default: false},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      files: {form: "...FILE"},
    },
  )?
  if opts.help { gnu.help(USAGE); return }
  if opts.version { gnu.write_text("od 0.13.0\n"); return }
  let address = if opts.address == "none" { "n" } else { opts.address }
  if address == "" { gnu.error("Radix cannot be empty, and must be one of [o, d, x, n]"); exit 1 }
  if address not in ["d", "o", "x", "n"] { gnu.usage_error(f"invalid output address radix: {gnu.quote_value(address)}") }
  let endian_little = opts.endian.starts_with("l") or opts.endian == "native"
  let endian_big = opts.endian.starts_with("b")
  if ! endian_little and ! endian_big { gnu.usage_error(f"invalid endian {gnu.quote_value(opts.endian)}") }
  let skip = parse_nonnegative(opts.skip)
  let limit: Int? = if opts.read == "" { null } else { parse_nonnegative(opts.read) }
  let width = parse_nonnegative(opts.width)
  if skip == null or (opts.read != "" and limit == null) { gnu.usage_error("invalid byte count") }
  if width == null or (width ?? 0) < 1 { gnu.error(f"invalid -w argument {gnu.quote_value(opts.width)}"); exit 1 }
  var files = opts.files
  var traditional_skip: Int? = null
  var pseudo_start: Int? = null
  let traditional = opts.traditional
  let modern = uses_modern_option(argv)
  if traditional or ! modern {
    if files.len() == 1 {
      let candidate = files[0]
      if (traditional or candidate.starts_with("+")) and traditional_offset_candidate(candidate) {
        traditional_skip = check_traditional_offset(candidate)
        files = []
      }
    } else if files.len() == 2 {
      let first = files[0]
      let second = files[1]
      let should_parse = traditional or second.starts_with("+") or (second.byte_len() > 0 and second.byte_slice(0, 1) in ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"])
      if should_parse and traditional_offset_candidate(second) {
        let first_is_offset = traditional and traditional_offset_candidate(first)
        if first_is_offset {
          traditional_skip = check_traditional_offset(first)
          pseudo_start = check_traditional_offset(second)
          files = []
        } else {
          traditional_skip = check_traditional_offset(second)
          files = [first]
        }
      }
    } else if traditional and files.len() == 3 {
      let second = files[1]
      let third = files[2]
      if traditional_offset_candidate(second) and traditional_offset_candidate(third) {
        traditional_skip = check_traditional_offset(second)
        pseudo_start = check_traditional_offset(third)
        files = [files[0]]
      }
    }
    if traditional and files.len() > 1 {
      gnu.error(f"extra operand {gnu.quote(files[1])}")
      gnu.error("compatibility mode supports at most one file")
      exit 1
    }
  }
  var data: Bytes = b""
  let raw_args = cli.argv_bytes()
  if files.len() == 0 { data = read_file("-", b"-") } else {
    for name in files {
      let item = read_file(name, raw_for(argv, raw_args, name))
      data = bytes.concat([data, item])
    }
  }
  let start = traditional_skip ?? skip ?? 0
  let available = if start < data.len() { data.len() - start } else { 0 }
  let amount = if limit == null or (limit ?? 0) > available { available } else { limit ?? 0 }
  data = data.slice(start, length: amount)

  if opts.strings != "" {
    let minimum = if opts.strings == "" { 3 } else { parse_nonnegative(opts.strings) ?? 3 }
    let result = strings_output(data, start, if minimum == 0 { 1 } else { minimum }, opts.read != "", address, pseudo_start)
    gnu.write_text(result)
    return
  }

  var formats: List[Str] = []
  var parsing_options = true
  var arg_at = 0
  while arg_at < argv.len() {
    let arg = argv[arg_at]
    if parsing_options and arg == "--" { parsing_options = false } else if parsing_options and (arg == "-t" or arg == "--format") {
      if arg_at + 1 < argv.len() { formats += split_format_types(argv[arg_at + 1]); arg_at += 1 }
    } else if parsing_options and arg.starts_with("--format=") {
      formats += split_format_types(arg.byte_slice(9))
    } else if parsing_options and arg.starts_with("-t") and arg.byte_len() > 2 {
      formats += split_format_types(arg.byte_slice(2))
    } else if parsing_options {
      let short = short_format(arg)
      if short != null { formats += [short ?? ""] }
    }
    arg_at += 1
  }
  if "-f" in formats or "f8" in formats { gnu.usage_error("floating-point formats are not supported") }
  if formats.len() == 0 { formats = ["o2"] }
  var parsed_formats: List[Format] = []
  for spec in formats {
    let parsed = parse_type(spec)
    if parsed == null { gnu.usage_error(f"invalid type string {gnu.quote_value(spec)}") } else { parsed_formats += [parsed ?? {base: "o", size: 2, mode: "unsigned", ascii: false}] }
  }
  if parsed_formats.len() == 0 { parsed_formats = [{base: "o", size: 2, mode: "unsigned", ascii: false}] }
  let little = endian_little
  var line_width = width ?? 16
  var unit = 1
  for format in parsed_formats { if format.size > unit { unit = format.size } }
  if line_width % unit != 0 {
    let adjusted = unit
    eprint f"od: warning: invalid width {line_width}; using {adjusted} instead"
    line_width = adjusted
  }
  var out = ""
  var offset = 0
  var previous: Str? = null
  var star = false
  while offset < data.len() {
    let amount = if data.len() - offset < line_width { data.len() - offset } else { line_width }
    let shown_address = formatted_address(offset + start, address, if pseudo_start == null { null } else { (pseudo_start ?? 0) + offset })
    let head = if shown_address == "" { " " } else { f"{shown_address} " }
    var format_lines: List[Str] = []
    for format in parsed_formats {
      var values: List[Str] = []
      var at = offset
      while at < offset + amount {
        let value = if format.mode == "char" { char_word(data, at, false) } else if format.mode == "ascii" { char_word(data, at, true) } else { numeric_word(data, at, format, little) }
        values += [value]
        at += format.size
      }
      if format.ascii {
        var shown = ""
        for index in range(amount) {
          let byte = data.byte_at(offset + index) ?? 0
          shown = f"{shown}{if byte >= 32 and byte <= 126 { data[offset + index..offset + index + 1].utf8() ?? "" } else { "." }}"
        }
        let columns = line_width / format.size
        let field_area = columns * display_width(format) + columns - 1
        format_lines += [f"{pad_right(values.join(" "), field_area)}  >{shown}<"]
      } else {
        format_lines += [values.join(" ")]
      }
    }
    var body = ""
    let format_indent = if pseudo_start != null { pad_right("", if address == "n" { 10 } else { 18 }) } else if address == "n" { " " } else if address == "x" { "       " } else { "        " }
    for index in range(format_lines.len()) {
      body = if index == 0 { format_lines[index] } else { f"{body}\n{format_indent}{format_lines[index]}" }
    }
    let repeated = amount == line_width and previous == body
    if opts.duplicates or ! repeated {
      out = f"{out}{head}{body}\n"
      previous = if amount == line_width { body } else { null }
      star = false
    } else if ! star {
      out = f"{out}*\n"
      star = true
    }
    offset += line_width
  }
  let final_address = formatted_address(start + data.len(), address, if pseudo_start == null { null } else { (pseudo_start ?? 0) + data.len() })
  if final_address != "" { out = f"{out}{final_address}\n" }
  if data.len() == 0 and address == "n" and pseudo_start == null { out = "" }
  gnu.write_text(out)
  if let Err(failure) = io.flush_stdout() { gnu.error(gnu.strerror(failure)); exit 1 }
}
