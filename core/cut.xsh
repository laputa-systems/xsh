#!/bin/xsh
use lib.gnu
use lib.textio_a1 as tio

const USAGE = """Usage: cut OPTION... [FILE]...
Print selected parts of lines from each FILE to standard output.
  -b, --bytes=LIST       select only these bytes
  -c, --characters=LIST  select only these characters
  -d, --delimiter=DELIM  use DELIM instead of TAB for fields
  -f, --fields=LIST      select only these fields
  -F, --fields-merged=LIST select fields separated by whitespace
  -n, --no-partial       do not split multibyte characters
  -s, --only-delimited   do not print lines without delimiters
  -w, --whitespace-delimited[=KIND] use blanks as delimiters
  -z, --zero-terminated  use NUL as line delimiter
  -O, --output-delimiter=STRING use STRING between selected fields
      --complement       complement the selected bytes, characters or fields
      --help             display this help and exit
      --version          output version information and exit
"""

const MAX_POSITION = 9223372036854775807

type CutRange = {first: Int, last: Int}
type CutRecord = {data: Bytes, ended: Bool}
type CutSplit = {fields: List[Bytes], separated: Bool}
type CutUnit = {size: Int, text: Str}

pure parse_selection(spec: Str, fields: Bool) -> Result[List[CutRange], Str] {
  let label = if fields { "field" } else { "byte/character position" }
  let range_label = if fields { "field range" } else { "byte or character range" }
  let empty_error = if fields { "fields are numbered from 1" } else { "byte/character positions are numbered from 1" }
  var ranges: List[CutRange] = []

  for token in spec.split(",") {
    if token == "" { return Err(empty_error) }

    let dash = token.find("-")
    var first = 0
    var last = 0

    if dash == null {
      first = if rx"^[0-9]+$".matches(token) { token.parse_int() ?? -1 } else { -1 }
      last = first
      if first < 0 {
        return Err(if fields { f"invalid field value '{token}'" } else { f"invalid {label} '{token}'" })
      }
    } else {
      let at = dash ?? 0
      let left = token.byte_slice(0, length: at)
      let right = token.byte_slice(at + 1)

      if right.find("-") != null {
        return Err(f"invalid {range_label}")
      }
      if left == "" and right == "" {
        return Err(f"invalid range with no endpoint: {token}")
      }
      first = if left == "" { 1 } else if rx"^[0-9]+$".matches(left) { left.parse_int() ?? -1 } else { -1 }
      last = if right == "" { MAX_POSITION } else if rx"^[0-9]+$".matches(right) { right.parse_int() ?? -1 } else { -1 }
      if first < 0 {
        let value = if left == "" { right } else { left }
        return Err(if fields { f"invalid field value '{value}'" } else { f"invalid {label} '{value}'" })
      }
      if last < 0 {
        let value = if right == "" { left } else { right }
        return Err(if fields { f"invalid field value '{value}'" } else { f"invalid {label} '{value}'" })
      }
      if first > last { return Err("invalid decreasing range") }
    }

    if first == 0 or last == 0 { return Err(empty_error) }
    ranges += [{first: first, last: last}]
  }

  Ok(ranges)
}

pure in_selection(position: Int, ranges: List[CutRange]) -> Bool {
  for range in ranges {
    return true when position >= range.first and position <= range.last
  }
  false
}

pure selected(position: Int, ranges: List[CutRange], complement: Bool) -> Bool {
  let found = in_selection(position, ranges)
  if complement { ! found } else { found }
}

pure range_group(position: Int, ranges: List[CutRange]) -> Int {
  for index in range(ranges.len()) {
    let range = ranges[index]
    if position >= range.first and position <= range.last { return index }
  }
  -1
}

pure matching(data: Bytes, at: Int, pattern: Bytes) -> Bool {
  return false when pattern.len() == 0 or at + pattern.len() > data.len()
  data[at..at + pattern.len()] == pattern
}

pure utf8_boundary(data: Bytes, at: Int) -> Bool {
  var cursor = 0
  while cursor < at {
    cursor += utf8_unit(data, cursor).size
  }
  cursor == at
}

pure utf8_unit(data: Bytes, at: Int) -> CutUnit {
  let lead = data.byte_at(at) ?? 0
  let candidate = if lead >= 194 and lead <= 223 { 2 } else if lead >= 224 and lead <= 239 { 3 } else if lead >= 240 and lead <= 244 { 4 } else { 1 }
  let valid = candidate > 1 and at + candidate <= data.len() and (data[at..at + candidate].utf8() ?? "") != ""
  let size = if valid { candidate } else { 1 }
  {size: size, text: data[at..at + size].utf8() ?? ""}
}

pure records(data: Bytes, zero: Bool) -> List[CutRecord] {
  let ends = tio.line_ends(data, zero)
  var rows: List[CutRecord] = []
  var start = 0

  for end in ends {
    rows += [{data: data[start..end - 1], ended: true}]
    start = end
  }

  if start < data.len() {
    rows += [{data: data[start..], ended: false}]
  }

  rows
}

pure blank_size(data: Bytes, at: Int) -> Int {
  let byte = data.byte_at(at) ?? 0
  return 1 when byte in [9, 10, 11, 12, 13, 32]
  let unit = utf8_unit(data, at)
  return unit.size when unit.text in ["\u{00a0}", "\u{1680}", "\u{2000}", "\u{2001}", "\u{2002}", "\u{2003}", "\u{2004}", "\u{2005}", "\u{2006}", "\u{2008}", "\u{2009}", "\u{200a}", "\u{202f}", "\u{205f}", "\u{3000}"]
  0
}

pure trim_blanks(data: Bytes) -> Bytes {
  var start = 0
  var end = data.len()
  while start < end and blank_size(data, start) > 0 { start += blank_size(data, start) }
  var cursor = start
  var last_nonblank = start
  while cursor < end {
    let size = blank_size(data, cursor)
    if size == 0 {
      let unit = utf8_unit(data, cursor)
      cursor += unit.size
      last_nonblank = cursor
    } else {
      cursor += size
    }
  }
  end = last_nonblank
  data[start..end]
}

pure split_fields(data: Bytes, delimiter: Bytes, whitespace: Bool, trimmed: Bool, utf8: Bool) -> CutSplit {
  let input = if whitespace and trimmed { trim_blanks(data) } else { data }
  var fields: List[Bytes] = []
  var at = 0
  var start = 0
  var separated = false

  while at < input.len() {
    let size = if whitespace {
      blank_size(input, at)
    } else if matching(input, at, delimiter) and (! utf8 or utf8_boundary(input, at)) {
      delimiter.len()
    } else {
      0
    }

    if size == 0 {
      at += utf8_unit(input, at).size
    } else {
      fields += [input[start..at]]
      separated = true
      at += size

      if whitespace {
        while at < input.len() and blank_size(input, at) > 0 { at += blank_size(input, at) }
      }

      start = at
    }
  }

  fields += [input[start..]]
  {fields: fields, separated: separated}
}

pure join_bytes(parts: List[Bytes], separator: Bytes) -> Bytes {
  var output: List[Bytes] = []
  for index in range(parts.len()) {
    if index > 0 { output += [separator] }
    output += [parts[index]]
  }
  bytes.concat(output)
}

pure cut_fields(
  line: Bytes,
  ranges: List[CutRange],
  delimiter: Bytes,
  output_delimiter: Bytes,
  whitespace: Bool,
  trimmed: Bool,
  utf8: Bool,
  only: Bool,
  complement: Bool,
) -> Bytes {
  let split = split_fields(line, delimiter, whitespace, trimmed, utf8)

  if ! split.separated {
    return b"" when only
    return if whitespace and trimmed { trim_blanks(line) } else { line}
  }

  var selected_fields: List[Bytes] = []
  for index in range(split.fields.len()) {
    if selected(index + 1, ranges, complement) { selected_fields += [split.fields[index]] }
  }
  join_bytes(selected_fields, output_delimiter)
}

pure cut_units(
  line: Bytes,
  ranges: List[CutRange],
  complement: Bool,
  characters: Bool,
  no_partial: Bool,
  utf8: Bool,
  output_delimiter: Bytes,
  use_output_delimiter: Bool,
) -> Bytes {
  var chunks: List[Bytes] = []
  var at = 0
  var position = 1
  var gap = false
  var emitted = false
  var previous_group = -1

  while at < line.len() {
    let unit = if (characters and utf8) or (no_partial and utf8) { utf8_unit(line, at) } else { {size: 1, text: ""} }
    let size = unit.size
    var keep = false
    var range_id = -1

    if characters {
      keep = selected(position, ranges, complement)
      if keep and ! complement { range_id = range_group(position, ranges) }
    } else if no_partial and utf8 and size > 1 {
      var first = -1
      var last = -1
      var count = 0
      var last_group = -1
      for offset in range(size) {
        if selected(at + offset + 1, ranges, complement) {
          if first < 0 { first = offset }
          last = offset
          count += 1
          if ! complement { last_group = range_group(at + offset + 1, ranges) }
        }
      }
      keep = count > 0 and last == size - 1 and count == last - first + 1
      range_id = last_group
    } else {
      keep = selected(position, ranges, complement)
      if keep and ! complement { range_id = range_group(position, ranges) }
    }

    if keep {
      let new_range = ! complement and previous_group >= 0 and range_id >= 0 and previous_group != range_id
      if emitted and (gap or new_range) and use_output_delimiter { chunks += [output_delimiter] }
      chunks += [line[at..at + size]]
      emitted = true
      gap = false
      previous_group = range_id
    } else if emitted {
      gap = true
    }

    at += size
    position += if characters { 1 } else { size }
  }

  bytes.concat(chunks)
}

pure raw_files(argv: List[Str], raw: List[Bytes]) -> List[Bytes] {
  var files: List[Bytes] = []
  var index = 0
  var after_separator = false

  while index < argv.len() {
    let arg = argv[index]

    if ! after_separator and arg == "--" {
      after_separator = true
      index += 1
      continue
    }

    if ! after_separator and (arg in ["-b", "--bytes", "-c", "--characters", "-f", "--fields", "-F", "--fields-merged", "-d", "--delimiter", "-O", "--output-delimiter"] or arg.starts_with("--byt") or arg.starts_with("--char") or arg.starts_with("--fie") or arg.starts_with("--del") or arg.starts_with("--out")) {
      index += 2
      continue
    }

    if ! after_separator and arg.starts_with("-") and arg != "-" {
      index += 1
      continue
    }

    files += [raw[index]]
    index += 1
  }
  files
}

pure raw_option(argv: List[Str], raw: List[Bytes], short: Str, long_prefix: Str) -> Bytes? {
  var value: Bytes? = null
  var index = 0

  while index < argv.len() {
    let arg = argv[index]
    let equal = arg.find("=")

    if arg == short or (arg.starts_with("--") and arg.starts_with(long_prefix) and equal == null) {
      value = raw[index + 1]
      index += 2
    } else if arg.starts_with("--") and arg.starts_with(long_prefix) and equal != null {
      value = raw[index].slice((equal ?? 0) + 1)
      index += 1
    } else if ! arg.starts_with("--") and arg.starts_with(short) and arg != short {
      value = raw[index].slice(short.byte_len())
      index += 1
    } else {
      index += 1
    }
  }

  value
}

proc read_raw(raw: Bytes) [fs, error, io] -> Result[Bytes, Error] {
  return io.stdin_bytes() when raw == b"-"
  let target = Path.parse_bytes(raw)?
  target.read_bytes()
}

proc utf8_locale() [env] -> Bool {
  var value = ""
  for key in ["LC_ALL", "LC_CTYPE", "LANG"] {
    let found = env.get_or(key, "") ?? ""
    if found != "" { value = found; break }
  }
  let lower = value.lower()
  lower.find("utf-8") != null or lower.find("utf8") != null
}

type CutOptions = {
  bytes: List[Str],
  characters: List[Str],
  fields: List[Str],
  merged: List[Str],
  delimiter: Str?,
  output_delimiter: Str?,
  whitespace: Str,
  only: Bool,
  complement: Bool,
  no_partial: Bool,
  zero: Bool,
  help: Bool,
  version: Bool,
  files: List[Str],
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: CutOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      bytes: {form: "-b --bytes LIST", default: [], repeated: true},
      characters: {form: "-c --characters LIST", default: [], repeated: true},
      fields: {form: "-f --fields LIST", default: [], repeated: true},
      merged: {form: "-F LIST", default: [], repeated: true},
      delimiter: {form: "-d --delimiter CHAR"},
      output_delimiter: {form: "-O --output-delimiter STRING"},
      whitespace: {form: "-w --whitespace-delimited[=KIND]", default: "", optional_default: "untrimmed"},
      only: {form: "-s --only-delimited", default: false},
      complement: {form: "--complement", default: false},
      no_partial: {form: "-n --no-partial", default: false},
      zero: {form: "-z --zero-terminated", default: false},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      files: {form: "...FILE"},
    },
  )?

  if opts.help { gnu.help(USAGE); return }
  if opts.version { gnu.version("cut"); return }

  let mode_count = opts.bytes.len() + opts.characters.len() + opts.fields.len() + opts.merged.len()
  if mode_count == 0 {
    gnu.error("you must specify a list of bytes, characters, or fields")
    gnu.try_help()
    exit 1
  }
  if mode_count > 1 {
    gnu.error("only one list may be specified")
    gnu.try_help()
    exit 1
  }

  let is_bytes = opts.bytes.len() > 0
  let is_chars = opts.characters.len() > 0
  let is_fields = opts.fields.len() > 0 or opts.merged.len() > 0
  let merged = opts.merged.len() > 0
  let field_mode = is_fields
  var whitespace_enabled = opts.whitespace != ""
  var whitespace_trimmed = opts.whitespace in ["trimmed", "tri"]

  for arg in argv {
    if arg == "-w" or arg == "--whitespace-delimited" or arg.starts_with("--whitespace-delimited=") {
      whitespace_enabled = true
      whitespace_trimmed = whitespace_trimmed or arg == "--whitespace-delimited=" or arg == "--whitespace-delimited=trimmed" or arg == "--whitespace-delimited=tri"
    }
  }

  let spec = if is_bytes { opts.bytes[0] } else if is_chars { opts.characters[0] } else if merged { opts.merged[0] } else { opts.fields[0] }
  let ranges = match parse_selection(spec, field_mode) {
    Ok(value) => value
    Err(message) => {
      gnu.error(message)
      gnu.try_help()
      exit 1
    }
  }

  if opts.only and ! field_mode {
    gnu.error("suppressing non-delimited lines makes sense\n\tonly when operating on fields")
    gnu.try_help()
    exit 1
  }
  if whitespace_enabled and ! field_mode {
    gnu.error("an input delimiter makes sense\n\tonly when operating on fields")
    gnu.try_help()
    exit 1
  }
  if opts.delimiter != null and ! field_mode {
    gnu.error("an input delimiter makes sense\n\tonly when operating on fields")
    gnu.try_help()
    exit 1
  }
  if whitespace_enabled and opts.delimiter != null and ! merged {
    gnu.error("-d and -w are mutually exclusive")
    gnu.try_help()
    exit 1
  }
  if opts.whitespace != "" and opts.whitespace not in ["untrimmed", "trimmed", "tri", ""] {
    gnu.error(f"invalid whitespace-delimited value {gnu.quote_value(opts.whitespace)}")
    gnu.try_help()
    exit 1
  }
  if whitespace_enabled and (is_bytes or is_chars) {
    gnu.error("an input delimiter makes sense\n\tonly when operating on fields")
    gnu.try_help()
    exit 1
  }

  let raw = cli.argv_bytes()
  let selected_delimiter = raw_option(argv, raw, "-d", "--del")
  let delimiter_value = selected_delimiter ?? bytes.from_text("\t")
  let delimiter = if opts.delimiter != null and delimiter_value.len() == 0 { b"\0" } else { delimiter_value }
  let utf8 = utf8_locale()

  if field_mode and opts.delimiter != null {
    let d = opts.delimiter ?? ""
    let raw_d = selected_delimiter ?? bytes.from_text(d)
    let decoded = raw_d.utf8() ?? ""
    let chars = decoded.count_chars()
    if (utf8 and decoded != "" and chars != 1) or (! utf8 and raw_d.len() > 1) {
      gnu.error("the delimiter must be a single character")
      gnu.try_help()
      exit 1
    }
  }

  let raw_output = raw_option(argv, raw, "-O", "--out")
  let use_output = opts.output_delimiter != null
  let output_override = raw_output ?? bytes.from_text(opts.output_delimiter ?? "")
  let zero = opts.zero
  let term = if zero { b"\0" } else { b"\n" }
  let whitespace = whitespace_enabled or (merged and opts.delimiter == null)
  let trimmed = whitespace_trimmed
  let field_output = if use_output { output_override } else if merged { b" " } else { delimiter }
  let unit_output = if use_output { output_override } else { b"" }
  let raw_paths = raw_files(argv, raw)
  let paths = if raw_paths.len() == 0 { [b"-"] } else { raw_paths }
  var failed = false

  for name in paths {
    if name != b"-" {
      if let Ok(target) = Path.parse_bytes(name) {
        if let Ok(meta) = target.metadata() {
          if meta.mode / 4096 % 16 == 4 {
            gnu.error(f"{gnu.quote_bytes(name, always: false)}: Is a directory")
            failed = true
            continue
          }
        }
      }
    }

    guard let data = read_raw(name) else { |failure|
      if gnu.errno(failure) == 5 {
        gnu.error(gnu.strerror(failure))
      } else {
        gnu.error(f"{gnu.quote_bytes(name, always: false)}: {gnu.strerror(failure)}")
      }
      failed = true
      continue
    }

    for record in records(data, zero) {
      if field_mode and delimiter == term and ! zero {
        continue
      }

      let content = record.data
      var output = b""

      if field_mode {
        let field_delimiter = if whitespace and opts.delimiter == null { b" " } else { delimiter }
        let result = cut_fields(content, ranges, field_delimiter, field_output, whitespace and opts.delimiter == null, trimmed, utf8, opts.only, opts.complement)
        let separated = split_fields(content, field_delimiter, whitespace and opts.delimiter == null, trimmed, utf8).separated
        let is_record_delimiter = zero and delimiter == term and record.ended
        if is_record_delimiter and ! separated { output = content } else { output = result }
        if opts.only and ! separated and ! is_record_delimiter { continue }
      } else {
        output = cut_units(content, ranges, opts.complement, is_chars, opts.no_partial, utf8, unit_output, use_output)
      }

      gnu.write_bytes(bytes.concat([output, term]))
    }

    if field_mode and delimiter == b"\n" and ! zero {
      let rows = records(data, false)
      let had_separator = rows.len() > 1 or (rows.len() == 1 and rows[0].ended)

      if ! had_separator {
        if ! opts.only {
          gnu.write_bytes(bytes.concat([data, term]))
        }
      } else {
        var chosen: List[Bytes] = []
        for index in range(rows.len()) {
          if selected(index + 1, ranges, opts.complement) { chosen += [rows[index].data] }
        }
        if chosen.len() > 0 {
          gnu.write_bytes(bytes.concat([join_bytes(chosen, field_output), term]))
        } else if ! opts.only and data.len() > 0 {
          gnu.write_bytes(term)
        }
      }
    }
  }

  if failed { exit 1 }
}
