#!/bin/xsh
use lib.gnu
use lib.text_a2 as text

type Options = {bytes: List[Str], characters: List[Str], fields: List[Str], merged: List[Str], whitespace: Str?, delimiter: Str?, output: Str?, complement: Bool, separated: Bool, no_split: Bool, zero: Bool, help: Bool, version: Bool, paths: List[Str]}
type Span = {first: Int, last: Int}

proc bound(raw: Str, remaining: Str, fields: Bool) -> Int {
  if ! rx"^[0-9]+$".matches(raw) {
    var at = 0
    while at < remaining.byte_len() and rx"^[0-9]$".matches(remaining.byte_slice(at, length: 1)) { at += 1 }
    gnu.usage_error(f"invalid {if fields { "field value" } else { "byte/character position" }} {gnu.quote_value(remaining.byte_slice(at))}")
  }
  guard let value = raw.parse_int() else { |_|
    gnu.usage_error(f"{if fields { "field number" } else { "byte/character offset" }} {gnu.quote_value(raw)} is too large")
    exit 1
  }
  value
}

proc ranges(spec: Str, fields: Bool) -> List[Span] {
  if spec == "" { gnu.usage_error(if fields { "fields are numbered from 1" } else { "byte/character positions are numbered from 1" }) }
  let spans: List[Span] = collect {
    for word in spec.replace(",", with: " ").fields() {
      let parts = word.split("-")
      if parts.len() > 2 { gnu.usage_error(if fields { "invalid field range" } else { "invalid byte or character range" }) }
      if word == "-" { gnu.usage_error("invalid range with no endpoint: -") }
      let first = if parts[0] == "" { 1 } else { bound(parts[0], word, fields) }
      # Only the start of a range (or a lone number) is numbered from 1; a zero end is a decreasing range instead.
      if first == 0 { gnu.usage_error(if fields { "fields are numbered from 1" } else { "byte/character positions are numbered from 1" }) }
      let last =if parts.len() == 1 { first } else if parts[1] == "" { 9223372036854775807 } else { bound(parts[1], parts[1], fields) }
      if first > last { gnu.usage_error("invalid decreasing range") }
      yield {first: first, last: last}
    }
  }
  if spans.is_empty() { gnu.usage_error(if fields { "fields are numbered from 1" } else { "byte/character positions are numbered from 1" }) }
  var merged: List[Span] = []
  for span in spans |> sort-by .first {
    if ! merged.is_empty() and span.first <= merged[-1].last {
      let previous = merged[-1]
      merged = [@merged[..merged.len() - 1], {first: previous.first, last: if span.last > previous.last { span.last } else { previous.last }}]
    } else { merged += [span] }
  }
  merged
}

enum Charset { SingleByte, Utf8, Gb18030 }

proc charset() -> Charset {
  var locale = ""
  for name in ["LC_ALL", "LC_CTYPE", "LANG"] {
    let value = env.get_or(name, "") ?? ""
    if value != "" { locale = value; break }
  }
  if locale.lower().find("utf-8") != null or locale.lower().find("utf8") != null { Utf8 } else if locale.lower().find("gb18030") != null { Gb18030 } else { SingleByte }
}

pure char_width(line: Bytes, at: Int, encoding: Charset) -> Int {
  if encoding == SingleByte { return 1 }
  let first = line.byte_at(at) ?? 0
  if encoding == Gb18030 {
    let second = line.byte_at(at + 1) ?? 0
    let third = line.byte_at(at + 2) ?? 0
    let fourth = line.byte_at(at + 3) ?? 0
    if first >= 129 and first <= 254 {
      if second >= 64 and second <= 254 and second != 127 { return 2 }
      if second >= 48 and second <= 57 and third >= 129 and third <= 254 and fourth >= 48 and fourth <= 57 {
        let pointer = ((first - 129) * 10 + second - 48) * 1260 + (third - 129) * 10 + fourth - 48
        return 4 when pointer < 39420 or (pointer >= 189000 and pointer <= 1237575)
      }
    }
    return 1
  }
  let lead = line.byte_at(at) ?? 0
  let width = if lead >= 194 and lead <= 223 { 2 } else if lead >= 224 and lead <= 239 { 3 } else if lead >= 240 and lead <= 244 { 4 } else { 1 }
  if at + width <= line.len() and (line[at..at + width].utf8() ?? "") != "" { width } else { 1 }
}

pure blank_size(data: Bytes, at: Int, unicode: Bool) -> Int {
  if unicode { text.blank_size(data, at) } else if (data.byte_at(at) ?? -1) in [9, 32] { 1 } else { 0 }
}

pure selected(index: Int, spans: List[Span], complement: Bool) -> Bool {
  var found = false
  for span in spans { if index >= span.first and index <= span.last { found = true; break } }
  if complement { ! found } else { found }
}

proc main(...argv: List[Bytes]) {
  let arguments = text.normalize_arguments(argv, ["-d", "--delimiter", "-O", "--output-delimiter"])
  let opts: Options = cli.applet(arguments.values, {
    gnu: {status: 1},
    bytes: {form: "-b --bytes LIST", repeated: true},
    characters: {form: "-c --characters LIST", repeated: true},
    fields: {form: "-f --fields LIST", repeated: true},
    merged: {form: "-F LIST", repeated: true},
    whitespace: {form: "-w --whitespace-delimited[=MODE]", optional_default: "untrimmed"},
    delimiter: {form: "-d --delimiter DELIM"},
    output: {form: "-O --output-delimiter STRING"},
    complement: {form: "--complement", default: false},
    separated: {form: "-s --only-delimited", default: false},
    no_split: {form: "-n --no-partial", default: false},
    zero: {form: "-z --zero-terminated", default: false},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    paths: {form: "...FILE"},
  })?
  if opts.help { gnu.help("Usage: cut OPTION... [FILE]...\nPrint selected parts of lines.\n  -b, --bytes=LIST\n  -c, --characters=LIST\n  -f, --fields=LIST\n  -d, --delimiter=DELIM\n  -s, --only-delimited\n  -z, --zero-terminated"); return }
  if opts.version { gnu.version("cut"); return }
  let modes = opts.bytes.len() + opts.characters.len() + opts.fields.len() + opts.merged.len()
  if modes == 0 { gnu.usage_error("you must specify a list of bytes, characters, or fields") }
  if modes > 1 { gnu.usage_error("only one list may be specified") }
  let fields = ! opts.fields.is_empty() or ! opts.merged.is_empty()
  let encoding = charset()
  if opts.whitespace != null and opts.delimiter != null { gnu.usage_error("-d and -w are mutually exclusive") }
  if ! fields and opts.separated { gnu.usage_error("suppressing non-delimited lines makes sense\n\tonly when operating on fields") }
  if ! fields and (opts.delimiter != null or opts.whitespace != null) { gnu.usage_error("an input delimiter makes sense\n\tonly when operating on fields") }
  let spans = ranges(if fields { opts.fields.get(0) ?? opts.merged.get(0) ?? "" } else { opts.bytes.get(0) ?? opts.characters.get(0) ?? "" }, fields)
  let delimiter_text = opts.delimiter ?? "\t"
  let input_delimiter = text.argument_bytes(arguments, delimiter_text)
  if delimiter_text != "" and char_width(input_delimiter, 0, encoding) != input_delimiter.len() { gnu.usage_error("the delimiter must be a single character") }
  let delimiter_bytes = if delimiter_text == "" { b"\0" } else { input_delimiter }
  let delimiter = delimiter_bytes.byte_at(0) ?? 9
  let whitespace = opts.whitespace != null or (! opts.merged.is_empty() and opts.delimiter == null)
  let trimmed = ! opts.merged.is_empty() or (opts.whitespace != null and opts.whitespace != "untrimmed")
  if let mode = opts.whitespace {
    if mode != "untrimmed" and ! "trimmed".starts_with(mode) { gnu.usage_error(f"invalid argument {gnu.quote_value(mode)} for '--whitespace-delimited'") }
  }
  let joiner = if let value = opts.output { if value == "" { b"\0" } else { text.argument_bytes(arguments, value) } } else if ! opts.merged.is_empty() { b" " } else { delimiter_bytes }
  let mark = if opts.zero { b"\0" } else { b"\n" }
  let unicode = encoding != SingleByte
  let character_positions = ! opts.characters.is_empty() and unicode
  let multibyte = (opts.no_split or character_positions) and unicode
  var failed = false
  let paths = if opts.paths.is_empty() { [b"-"] } else { text.argument_bytes_list(arguments, opts.paths) }
  for name in paths {
    guard let data = text.read_operand_bytes(name) else { |failure|
      text.name_error_bytes(name, failure); failed = true; continue
    }
    let final_delimiter = fields and ! data.is_empty() and data[data.len() - 1..] == mark and delimiter == (if opts.zero { 0 } else { 10 })
    let records = if fields and delimiter == (if opts.zero { 0 } else { 10 }) { if data.is_empty() { [] } else { [if data[data.len() - 1..] == mark { data[..data.len() - 1] } else { data }] } } else { text.records(data, if opts.zero { 0 } else { 10 }) }
    for raw_line in records {
      var line = raw_line
      if whitespace and trimmed {
        var begin = 0
        var end = line.len()
        while begin < end and blank_size(line, begin, unicode) > 0 { begin += blank_size(line, begin, unicode) }
        var cursor = begin
        end = begin
        while cursor < line.len() {
          let size = if unicode { char_width(line, cursor, encoding) } else { 1 }
          if blank_size(line, cursor, unicode) == 0 { end = cursor + size }
          cursor += size
        }
        line = line[begin..end]
      }
      var chunks: List[Bytes] = []
      if fields {
        var parts: List[Bytes] = []
        var start = 0
        var at = 0
        while at < line.len() {
          let separator = if whitespace { blank_size(line, at, unicode) } else if line[at..at + delimiter_bytes.len()] == delimiter_bytes { delimiter_bytes.len() } else { 0 }
          if separator > 0 {
            parts += [line[start..at]]
            at += separator
            if whitespace or ! opts.merged.is_empty() {
              while at < line.len() {
                let size = if whitespace { blank_size(line, at, unicode) } else if line[at..at + delimiter_bytes.len()] == delimiter_bytes { delimiter_bytes.len() } else { 0 }
                break when size == 0
                at += size
              }
            }
            start = at
          } else { at += if unicode { char_width(line, at, encoding) } else { 1 } }
        }
        if parts.is_empty() and ! final_delimiter {
          if ! opts.separated { gnu.write_bytes(bytes.concat([line, mark])) }
          continue
        }
        parts += [line[start..]]
        for item in parts |> enumerate() {
          if selected(item.index + 1, spans, opts.complement) {
            if ! chunks.is_empty() { chunks += [joiner] }
            chunks += [item.value]
          }
        }
      } else {
        var previous = false
        var previous_span = -1
        var at = 0
        var character_index = 1
        while at < line.len() {
          let width = if multibyte { char_width(line, at, encoding) } else { 1 }
          let position = if character_positions { character_index } else { at + width }
          var take = selected(position, spans, opts.complement)
          if multibyte and ! character_positions {
            var begun = false
            for position in range(at + 1, at + width + 1) {
              let included = selected(position, spans, opts.complement)
              if included { begun = true } else if begun { take = false }
            }
          }
          var span_index = -1
          for item in spans |> enumerate() {
            if position >= item.value.first and position <= item.value.last { span_index = item.index; break }
          }
          if take {
            if opts.output != null and (! previous or (! opts.complement and span_index != previous_span)) and ! chunks.is_empty() { chunks += [joiner] }
            chunks += [line[at..at + width]]
          }
          previous = take; previous_span = span_index; at += width; character_index += 1
        }
      }
      if fields and opts.separated and delimiter == (if opts.zero { 0 } else { 10 }) and chunks.is_empty() { continue }
      gnu.write_bytes(bytes.concat([@chunks, mark]))
    }
  }
  exit text.finish(failed)
}
