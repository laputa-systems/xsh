#!/bin/xsh
use lib.gnu
use lib.text_a2 as text

type Options = {bytes: Str, characters: Str, fields: Str, merged: Str, whitespace: Str?, delimiter: Str?, output: Str?, complement: Bool, separated: Bool, no_split: Bool, zero: Bool, help: Bool, version: Bool, paths: List[Str]}
type Span = {first: Int, last: Int}

proc ranges(spec: Str) -> List[Span] {
  if spec == "" { gnu.usage_error("missing list of fields, bytes, or characters") }
  let spans: List[Span] = collect {
    for word in spec.replace(",", with: " ").fields() {
      let parts = word.split("-")
      if parts.len() > 2 or word == "-" { gnu.usage_error("invalid range with no endpoint: -") }
      let first = if parts[0] == "" { 1 } else { parts[0].parse_int() ?? -1 }
      let last = if parts.len() == 1 { first } else if parts[1] == "" { 9223372036854775807 } else { parts[1].parse_int() ?? -1 }
      if first < 0 or last < 0 { gnu.usage_error(f"invalid byte, character or field list: {gnu.quote_value(spec)}") }
      if first <= 0 or last <= 0 { gnu.usage_error("fields and positions are numbered from 1") }
      if first > last { gnu.usage_error("invalid decreasing range") }
      yield {first: first, last: last}
    }
  }
  if spans.is_empty() { gnu.usage_error("missing list of fields, bytes, or characters") }
  var merged: List[Span] = []
  for span in spans |> sort-by .first {
    if ! merged.is_empty() and span.first <= merged[-1].last {
      let previous = merged[-1]
      merged = [@merged[..merged.len() - 1], {first: previous.first, last: if span.last > previous.last { span.last } else { previous.last }}]
    } else { merged += [span] }
  }
  merged
}

proc utf8_locale() -> Bool {
  var locale = ""
  for name in ["LC_ALL", "LC_CTYPE", "LANG"] {
    let value = env.get_or(name, "") ?? ""
    if value != "" { locale = value; break }
  }
  locale.lower().find("utf-8") != null or locale.lower().find("utf8") != null
}

pure char_width(line: Bytes, at: Int) -> Int {
  let lead = line.byte_at(at) ?? 0
  let width = if lead >= 194 and lead <= 223 { 2 } else if lead >= 224 and lead <= 239 { 3 } else if lead >= 240 and lead <= 244 { 4 } else { 1 }
  if at + width <= line.len() and (line[at..at + width].utf8() ?? "") != "" { width } else { 1 }
}

pure selected(index: Int, spans: List[Span], complement: Bool) -> Bool {
  var found = false
  for span in spans { if index >= span.first and index <= span.last { found = true; break } }
  if complement { ! found } else { found }
}

proc main(...argv: List[Str]) {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 1},
    bytes: {form: "-b --bytes LIST", default: "", conflicts: ["characters", "fields", "merged", "whitespace"]},
    characters: {form: "-c --characters LIST", default: "", conflicts: ["bytes", "fields", "merged", "whitespace"]},
    fields: {form: "-f --fields LIST", default: "", conflicts: ["bytes", "characters", "merged"]},
    merged: {form: "-F --fields-merged LIST", default: "", conflicts: ["bytes", "characters", "fields"]},
    whitespace: {form: "-w --whitespace-delimited[=MODE]", optional_default: "untrimmed", conflicts: ["delimiter", "bytes", "characters"]},
    delimiter: {form: "-d --delimiter DELIM", conflicts: ["whitespace"]},
    output: {form: "-O --output-delimiter STRING"},
    complement: {form: "--complement", default: false},
    separated: {form: "-s --only-delimited", default: false},
    no_split: {form: "-n", default: false},
    zero: {form: "-z --zero-terminated", default: false},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    paths: {form: "...FILE"},
  })?
  if opts.help { gnu.help("Usage: cut OPTION... [FILE]...\nPrint selected parts of lines.\n  -b, --bytes=LIST\n  -c, --characters=LIST\n  -f, --fields=LIST\n  -d, --delimiter=DELIM\n  -s, --only-delimited\n  -z, --zero-terminated"); return }
  if opts.version { gnu.version("cut"); return }
  let fields = opts.fields != "" or opts.merged != ""
  if ! fields and opts.bytes == "" and opts.characters == "" { gnu.usage_error("you must specify a list of bytes, characters, or fields") }
  if ! fields and opts.separated { gnu.usage_error("suppressing non-delimited lines makes sense\n\tonly when operating on fields") }
  if ! fields and opts.delimiter != null { gnu.usage_error("an input delimiter may be specified only when operating on fields") }
  if (opts.delimiter ?? "\t").count_chars() > 1 or (bytes.from_text(opts.delimiter ?? "\t").len() > 1 and ! utf8_locale()) { gnu.usage_error("the delimiter must be a single character") }
  let spans = ranges(if fields { if opts.fields != "" { opts.fields } else { opts.merged } } else if opts.bytes != "" { opts.bytes } else { opts.characters })
  let delimiter_text = opts.delimiter ?? "\t"
  let delimiter_bytes = if delimiter_text == "" { b"\0" } else { bytes.from_text(delimiter_text) }
  let delimiter = delimiter_bytes.byte_at(0) ?? 9
  let whitespace = opts.whitespace != null or (opts.merged != "" and opts.delimiter == null)
  let trimmed = opts.merged != "" or (opts.whitespace != null and opts.whitespace != "untrimmed")
  if let mode = opts.whitespace {
    if mode != "untrimmed" and ! "trimmed".starts_with(mode) { gnu.usage_error(f"invalid argument {gnu.quote_value(mode)} for '--whitespace-delimited'") }
  }
  let joiner = if let value = opts.output { if value == "" { b"\0" } else { bytes.from_text(value) } } else if opts.merged != "" { b" " } else { delimiter_bytes }
  let mark = if opts.zero { b"\0" } else { b"\n" }
  let multibyte = opts.no_split and utf8_locale()
  var failed = false
  for name in if opts.paths.is_empty() { ["-"] } else { opts.paths } {
    guard let data = gnu.read_operand(name) else { |failure|
      gnu.name_error(name, failure); failed = true; continue
    }
    let final_delimiter = fields and ! data.is_empty() and data[data.len() - 1..] == mark and delimiter == (if opts.zero { 0 } else { 10 })
    let records = if fields and delimiter == (if opts.zero { 0 } else { 10 }) { if data.is_empty() { [] } else { [if data[data.len() - 1..] == mark { data[..data.len() - 1] } else { data }] } } else { text.records(data, if opts.zero { 0 } else { 10 }) }
    for raw_line in records {
      var line = raw_line
      if whitespace and trimmed {
        var begin = 0
        var end = line.len()
        while begin < end and line.byte_at(begin) in [9, 32] { begin += 1 }
        while end > begin and line.byte_at(end - 1) in [9, 32] { end -= 1 }
        line = line[begin..end]
      }
      var chunks: List[Bytes] = []
      if fields {
        var parts: List[Bytes] = []
        var start = 0
        var at = 0
        while at < line.len() {
          let separator = if whitespace { line.byte_at(at) in [9, 32] } else { line[at..at + delimiter_bytes.len()] == delimiter_bytes }
          if separator {
            parts += [line[start..at]]
            at += if whitespace { 1 } else { delimiter_bytes.len() }
            if whitespace or opts.merged != "" {
              while at < line.len() and (if whitespace { line.byte_at(at) in [9, 32] } else { line[at..at + delimiter_bytes.len()] == delimiter_bytes }) { at += if whitespace { 1 } else { delimiter_bytes.len() } }
            }
            start = at
          } else { at += 1 }
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
        while at < line.len() {
          let width = if multibyte { char_width(line, at) } else { 1 }
          var take = selected(at + width, spans, opts.complement)
          if multibyte {
            var begun = false
            for position in range(at + 1, at + width + 1) {
              let included = selected(position, spans, opts.complement)
              if included { begun = true } else if begun { take = false }
            }
          }
          var span_index = -1
          for item in spans |> enumerate() {
            if at + width >= item.value.first and at + width <= item.value.last { span_index = item.index; break }
          }
          if take {
            if opts.output != null and (! previous or (! opts.complement and span_index != previous_span)) and ! chunks.is_empty() { chunks += [joiner] }
            chunks += [line[at..at + width]]
          }
          previous = take; previous_span = span_index; at += width
        }
      }
      if fields and opts.separated and delimiter == (if opts.zero { 0 } else { 10 }) and chunks.is_empty() { continue }
      gnu.write_bytes(bytes.concat([@chunks, mark]))
    }
  }
  exit text.finish(failed)
}
