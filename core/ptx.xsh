#!/bin/xsh
use lib.gnu

const USAGE = """Usage: ptx [OPTION]... [INPUT]...   (without -G)
  or:  ptx -G [OPTION]... [INPUT [OUTPUT]]
Output a permuted index, including context, of the words in the input files.

With no FILE, or when FILE is -, read standard input.

Mandatory arguments to long options are mandatory for short options too.
  -A, --auto-reference           output automatically generated references
  -G, --traditional              behave more like System V 'ptx'
  -F, --flag-truncation=STRING   use STRING for flagging line truncations.
                                 The default is '/'
  -M, --macro-name=STRING        macro name to use instead of 'xx'
  -O, --format=roff              generate output as roff directives
  -R, --right-side-refs          put references at right, not counted in -w
  -S, --sentence-regexp=REGEXP   for end of lines or end of sentences
  -T, --format=tex               generate output as TeX directives
  -W, --word-regexp=REGEXP       use REGEXP to match each keyword
  -b, --break-file=FILE          word break characters in this FILE
  -f, --ignore-case              fold lower case to upper case for sorting
  -g, --gap-size=NUMBER          gap size in columns between output fields
  -i, --ignore-file=FILE         read ignore word list from FILE
  -o, --only-file=FILE           read only word list from this FILE
  -r, --references               first field of each line is a reference
  -t, --typeset-mode             change the default width from 72 to 100
  -w, --width=NUMBER             output width in columns, reference excluded
      --help        display this help and exit
      --version     output version information and exit
"""

type PtxOptions = {
  auto_reference: Bool,
  traditional: Bool,
  truncation: Str,
  macro_name: Str,
  format: Str,
  roff: Bool,
  tex: Bool,
  right_refs: Bool,
  sentence: Str?,
  word: Str?,
  break_file: Str?,
  ignore_case: Bool,
  gap: Str,
  ignore_file: Str?,
  only_file: Str?,
  references: Bool,
  typeset: Bool,
  width: Str,
  help: Bool,
  version: Bool,
  files: List[Str],
}

# One keyword occurrence: the sort key, which input and line, and the keyword's
# byte span in that line.
type Occurrence = {key: Str, file: Int, line: Int, from: Int, to: Int}

# A half-open range of character indexes.
type Span = {s: Int, e: Int}

type Chunks = {head: Str, before: Str, keyword: Str, after: Str, tail: Str}

type Layout = {format: Str, width: Int, gap: Int, truncation: Str, macro_name: Str, refs: Bool, right: Bool}

const WORD_CLASS = "[A-Za-z0-9_\\x{00C0}-\\x{02AF}\\x{0370}-\\x{1FFF}\\x{2C00}-\\x{D7FF}\\x{FF10}-\\x{FFEF}]+"
const ALPHA_CLASS = "[A-Za-z\\x{00C0}-\\x{02AF}\\x{0370}-\\x{1FFF}\\x{2C00}-\\x{D7FF}\\x{FF21}-\\x{FFEF}]"
const SPACES = [" ", "\t", "\n", "\r", "\u{b}", "\u{c}", "\u{85}", "\u{a0}", "\u{2028}", "\u{2029}", "\u{3000}"]
const REGEX_SPECIAL = "\\.+*?()|[]{}^$#&-~"

pure is_space(c: Str) -> Bool {
  c in SPACES
}

# N spaces.
pure spaces(count: Int) -> Str {
  var out = ""
  var chunk = " "
  var left = count

  while left > 0 {
    if left % 2 == 1 {
      out = out + chunk
    }

    chunk = chunk + chunk
    left = left / 2
  }

  out
}

pure shorter(a: Int, b: Int) -> Int {
  if a > b { a - b } else { 0 }
}

pure smaller(a: Int, b: Int) -> Int {
  if a < b { a } else { b }
}

# Whitespace trimmed from both ends; a range of only whitespace collapses onto
# its start.
pure trim_span(cs: List[Str], range: Span) -> Span {
  var start = range.s
  var end = range.e

  while start < end and is_space(cs[start]) {
    start += 1
  }

  while range.s < end and is_space(cs[end - 1]) {
    end -= 1
  }

  {s: smaller(start, end), e: end}
}

# Move the start past a word it cuts in half.
pure align_start(cs: List[Str], range: Span) -> Span {
  var start = range.s

  return range when start == range.e or start == 0 or is_space(cs[start]) or is_space(cs[start - 1])

  while start < range.e and ! is_space(cs[start]) {
    start += 1
  }

  {s: start, e: range.e}
}

# Pull the end back off a word it cuts in half.
pure align_end(cs: List[Str], range: Span) -> Span {
  var end = range.e

  return range when range.s == end or end == cs.len() or is_space(cs[end - 1]) or is_space(cs[end])

  while range.s < end and ! is_space(cs[end - 1]) {
    end -= 1
  }

  {s: range.s, e: end}
}

# At most WIDTH characters from the right-hand end of RANGE, on whole words.
pure window_ending(cs: List[Str], range: Span, width: Int) -> Span {
  let end = trim_span(cs, range).e

  trim_span(cs, align_start(cs, {s: shorter(end, width), e: end}))
}

# At most WIDTH characters from the left-hand end of RANGE, on whole words; the
# start stays put because the after field butts against the keyword.
pure window_starting(cs: List[Str], range: Span, width: Int) -> Span {
  let start = range.s
  let end = align_end(cs, {s: start, e: smaller(range.e, start + width)}).e

  {s: start, e: trim_span(cs, {s: start, e: end}).e}
}

pure text_of(cs: List[Str], range: Span) -> Str {
  cs[range.s..range.e].join("")
}

pure span_len(range: Span) -> Int {
  range.e - range.s
}

# Lay the keyword and its surroundings out inside the line width: the
# context fields at each side, plus the head and tail that wrap around.
pure make_chunks(layout: Layout, before_cs: List[Str], keyword: Str, after_cs: List[Str]) -> Chunks {
  let half = layout.width / 2
  let max_before = shorter(half, layout.gap)
  let max_after = shorter(half, 2 * layout.truncation.count_chars() + keyword.count_chars() + 1)
  let before = window_ending(before_cs, {s: 0, e: before_cs.len()}, max_before)
  let after = window_starting(after_cs, {s: 0, e: after_cs.len()}, max_after)
  let max_tail = shorter(shorter(max_before, span_len(before)), layout.gap)
  let tail_start = trim_span(after_cs, {s: after.e, e: after_cs.len()}).s
  var tail = window_starting(after_cs, {s: tail_start, e: after_cs.len()}, max_tail)

  if span_len(tail) > 2 and is_space(after_cs[tail.e - 2]) and ! is_space(after_cs[tail.e - 1]) {
    tail = trim_span(after_cs, {s: tail.s, e: tail.e - 1})
  }

  let after_bytes = text_of(after_cs, after).byte_len()
  let max_head = shorter(shorter(max_after, after_bytes), layout.gap)
  let head = window_ending(before_cs, {s: 0, e: before.s}, max_head)
  var head_text = text_of(before_cs, head)
  var before_text = text_of(before_cs, before)
  var after_text = text_of(after_cs, after)
  var tail_text = text_of(after_cs, tail)

  if layout.format != "tex" {
    if after.e != after_cs.len() {
      if span_len(tail) == 0 {
        after_text = after_text + layout.truncation
      } else if tail.e != after_cs.len() {
        tail_text = tail_text + layout.truncation
      }
    }

    if before.s != 0 {
      if span_len(head) == 0 {
        before_text = layout.truncation + before_text
      } else if head.s != 0 {
        head_text = layout.truncation + head_text
      }
    }
  }

  {head: head_text, before: before_text, keyword: keyword, after: after_text, tail: tail_text}
}

pure tex_field(text: Str) -> Str {
  text.replace("\\", "\u{1}").replace("$", "\\$").replace("%", "\\%").replace("#", "\\#").replace("&", "\\&").replace("_", "\\_").replace("}", "$\\}$").replace("{", "$\\{$").replace("\u{1}", "\\backslash{}")
}

pure roff_field(text: Str) -> Str {
  text.replace("\"", "\"\"")
}

pure join_fields(first: Str, second: Str) -> Str {
  return second when first == ""
  return first when second == ""

  f"{first} {second}"
}

pure render(layout: Layout, chunks: Chunks, reference: Str) -> Str {
  if layout.format == "tex" {
    let base = f"\\{layout.macro_name} {{{tex_field(chunks.tail)}}}{{{tex_field(chunks.before)}}}{{{tex_field(chunks.keyword)}}}{{{tex_field(chunks.after)}}}{{{tex_field(chunks.head)}}}"

    return if layout.refs { f"{base}{{{tex_field(reference)}}}" } else { base }
  }

  if layout.format == "roff" {
    let base = f".{layout.macro_name} \"{roff_field(chunks.tail)}\" \"{roff_field(chunks.before)}\" \"{roff_field(chunks.keyword)}{roff_field(chunks.after)}\" \"{roff_field(chunks.head)}\""

    return if layout.refs { f"{base} \"{roff_field(reference)}\"" } else { base }
  }

  let left = join_fields(chunks.tail, chunks.before)
  let right = join_fields(chunks.after, chunks.head)
  let half = if layout.width / 2 > layout.gap { layout.width / 2 } else { layout.gap }
  let left_len = if layout.truncation != "" and left.find(layout.truncation) != null {
    left.byte_len() - layout.truncation.byte_len()
  } else {
    left.byte_len()
  }
  let pad = if left.byte_len() < half { half - left_len } else { 0 }
  let line = spaces(pad) + left + spaces(layout.gap) + chunks.keyword + right

  return line when ! layout.refs

  if layout.right { f"{line} {reference}" } else { f"{reference} {line}" }
}

# Text with invalid UTF-8 replaced by U+FFFD, as GNU ptx tolerates any bytes.
pure lossy(data: Bytes) -> Str {
  if let Ok(text) = data.utf8() {
    return text
  }

  var out = ""
  var at = 0
  let total = data.len()

  while at < total {
    let lead = data.byte_at(at) ?? 0
    let need = if lead < 128 { 1 } else if lead >= 194 and lead <= 223 { 2 } else if lead >= 224 and lead <= 239 { 3 } else if lead >= 240 and lead <= 244 { 4 } else { 0 }

    if need > 0 and at + need <= total {
      if let Ok(piece) = data[at..at + need].utf8() {
        out = out + piece
        at += need
        continue
      }
    }

    out = out + "\u{fffd}"
    at += 1
  }

  out
}

pure split_lines(text: Str) -> List[Str] {
  var parts = text.split("\n")

  if parts.len() > 0 and parts[parts.len() - 1] == "" {
    parts = parts[..parts.len() - 1]
  }

  [if part.ends_with("\r") { part.byte_slice(0, length: part.byte_len() - 1) } else { part } for part in parts]
}

# A trailing lone backslash is a literal one for GNU, but not for the regex
# engine used here.
pure patch_backslash(pattern: Str) -> Str {
  var count = 0
  var index = pattern.byte_len()

  while index > 0 and pattern.byte_slice(index - 1, length: 1) == "\\" {
    count += 1
    index -= 1
  }

  if count % 2 == 1 { pattern + "\\" } else { pattern }
}

pure escape_class(text: Str) -> Str {
  var out = ""

  for ch in [m.text for m in rx"(?s).".find(text)] {
    out = out + (if REGEX_SPECIAL.find(ch) != null { "\\" + ch } else { ch })
  }

  out
}

proc read_text(name: Str) [fs, process, env, error, io] -> Str {
  guard let data = gnu.read_operand(name) else { |failure|
    gnu.error(f"{gnu.quote(name)}: {gnu.strerror(failure)}")
    exit 1
  }

  lossy(data)
}

proc positive(text: Str, what: Str) [process, env] -> Int {
  let parsed = if rx"^[0-9]+$".matches(text) and text.byte_len() < 18 { text.parse_int() ?? 0 } else { 0 }

  if parsed < 1 {
    gnu.error(f"invalid {what}: {gnu.quote(text)}")
    exit 1
  }

  parsed
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: PtxOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      auto_reference: {form: "-A --auto-reference", default: false},
      traditional: {form: "-G --traditional", default: false},
      truncation: {form: "-F --flag-truncation STRING", default: "/"},
      macro_name: {form: "-M --macro-name STRING", default: "xx"},
      format: {form: "--format FORMAT", default: "", conflicts: ["roff", "tex"]},
      roff: {form: "-O", default: false, conflicts: ["format", "tex"]},
      tex: {form: "-T", default: false, conflicts: ["format", "roff"]},
      right_refs: {form: "-R --right-side-refs", default: false},
      sentence: {form: "-S --sentence-regexp REGEXP"},
      word: {form: "-W --word-regexp REGEXP"},
      break_file: {form: "-b --break-file FILE"},
      ignore_case: {form: "-f --ignore-case", default: false},
      gap: {form: "-g --gap-size NUMBER", default: ""},
      ignore_file: {form: "-i --ignore-file FILE"},
      only_file: {form: "-o --only-file FILE"},
      references: {form: "-r --references", default: false},
      typeset: {form: "-t --typeset-mode", default: false},
      width: {form: "-w --width NUMBER", default: ""},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      files: {form: "...FILE"},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }

  if opts.version {
    gnu.version("ptx")
    return
  }

  var format = if opts.traditional { "roff" } else { "dumb" }

  if opts.format != "" {
    var names = ["roff", "tex"]
    let found = [name for name in names if name.starts_with(opts.format)]

    if found.len() != 1 and ! (opts.format in names) {
      gnu.usage_error(f"invalid argument {gnu.quote(opts.format)} for '--format'\nValid arguments are:\n  - 'roff'\n  - 'tex'")
    }

    format = if opts.format in names { opts.format } else { found[0] }
  }

  if opts.roff {
    format = "roff"
  }

  if opts.tex {
    format = "tex"
  }

  let width = if opts.width != "" { positive(opts.width, "line width") } else if opts.typeset { 100 } else { 72 }
  let gap = if opts.gap != "" { positive(opts.gap, "gap width") } else { 3 }
  let context_pattern = if opts.traditional { "[^ \t\n]+" } else { WORD_CLASS }
  var sentence: Regex? = null

  if opts.sentence != null {
    let pattern = patch_backslash(opts.sentence ?? "")

    match regex.compile(pattern) {
      Ok(found) => {
        if found.matches("") {
          gnu.error("A regular expression cannot match a length zero string")
          exit 1
        }

        sentence = found
      }
      Err(failure) => {
        gnu.error(f"Invalid regexp: {failure.message}")
        exit 1
      }
    }
  }

  var inputs: List[Str] = opts.files
  var output = "-"

  if opts.traditional {
    if opts.files.len() > 2 {
      gnu.extra_operand(opts.files[2])
    }

    inputs = [opts.files.get(0) ?? "-"]
    output = opts.files.get(1) ?? "-"
  } else if inputs.len() == 0 {
    inputs = ["-"]
  }

  var only_words: List[Str] = []
  var ignore_words: List[Str] = []
  var break_chars = ""

  if opts.only_file != null {
    only_words = split_lines(read_text(opts.only_file ?? ""))
  }

  if opts.ignore_file != null {
    ignore_words = split_lines(read_text(opts.ignore_file ?? ""))
  }

  let word_override = opts.word ?? ""

  if opts.break_file != null and opts.word == null {
    break_chars = read_text(opts.break_file ?? "")

    if opts.traditional {
      break_chars = break_chars + " \t\n"
    }
  }

  var word_pattern = if word_override != "" {
    patch_backslash(word_override)
  } else if opts.break_file != null and opts.word == null {
    f"[^{escape_class(break_chars)}]+"
  } else if opts.traditional {
    "[^ \t\n]+"
  } else {
    WORD_CLASS
  }

  var files: List[List[Str]] = []

  for name in inputs {
    let text = read_text(name)

    if sentence != null {
      var pieces: List[Str] = []
      var at = 0

      for hit in (sentence ?? rx"x").find(text) {
        pieces += [text.byte_slice(at, length: hit.start - at)]
        at = hit.end
      }

      pieces += [text.byte_slice(at)]
      files += [[piece.replace("\n", " ") for piece in pieces if piece != ""]]
    } else {
      files += [split_lines(text)]
    }
  }

  var found: List[Occurrence] = []
  let word_regex = regex.compile(word_pattern)
  let context_regex = regex.compile(context_pattern)?
  let alpha = regex.compile(ALPHA_CLASS)?
  let default_words = word_pattern == WORD_CLASS

  if let Ok(words) = word_regex {
    for file in range(files.len()) {
      for index in range(files[file].len()) {
        let line = files[file][index]
        let first = context_regex.find(line)
        let ref_from = if first.len() > 0 { first[0].start } else { 0 }
        let ref_to = if first.len() > 0 { first[0].end } else { 0 }

        for hit in words.find(line) {
          var from = hit.start
          let to = hit.end

          if from == to {
            continue
          }

          if default_words {
            let inner = alpha.find(hit.text)

            if inner.len() == 0 {
              continue
            }

            from += inner[0].start
          }

          if opts.references and from == ref_from and to == ref_to {
            continue
          }

          let word = line.byte_slice(from, length: to - from)

          if opts.only_file != null and ! (word in only_words) {
            continue
          }

          if opts.ignore_file != null and word in ignore_words {
            continue
          }

          found += [{key: if opts.ignore_case { word.upper() } else { word }, file: file, line: index, from: from, to: to}]
        }
      }
    }
  }

  let ordered = found |> sort-by .key
  let layout_refs = opts.auto_reference or opts.references
  var line_width = width

  if ! opts.right_refs {
    var digits = 0
    var names = 0

    if opts.auto_reference {
      for item in ordered {
        let count = if item.line == 0 { 1 } else { f"{item.line}".byte_len() }

        if count > digits {
          digits = count
        }

        let shown = if inputs[item.file] == "-" { 0 } else { gnu.quote_maybe(inputs[item.file]).byte_len() }

        if shown > names {
          names = shown
        }
      }
    }

    if opts.auto_reference and ordered.len() > 0 {
      line_width = shorter(width, digits + names + 1)
    }
  }

  let layout: Layout = {
    format: format,
    width: line_width,
    gap: gap,
    truncation: opts.truncation,
    macro_name: opts.macro_name,
    refs: layout_refs,
    right: opts.right_refs,
  }

  var out: List[Str] = []

  for item in ordered {
    let line = files[item.file][item.line]
    let name = inputs[item.file]
    var reference = ""

    if opts.auto_reference {
      reference = if name == "-" { f":{item.line + 1}" } else { f"{gnu.quote_maybe(name)}:{item.line + 1}" }
    } else if opts.references {
      let first = context_regex.find(line)

      reference = if first.len() > 0 { first[0].text } else { ""}
    }

    let before_text = line.byte_slice(0, length: item.from)
    var trimmed = before_text

    if opts.references and reference != "" {
      while trimmed.starts_with(reference) {
        trimmed = trimmed.byte_slice(reference.byte_len())
      }
    }

    let lead = if opts.references { before_text.byte_len() - rx"^\s+".replace(trimmed, "").byte_len() } else { 0 }
    let before_cs = [m.text for m in rx"(?s).".find(before_text.byte_slice(lead))]
    let keyword = line.byte_slice(item.from, length: item.to - item.from)
    let after_cs = [m.text for m in rx"(?s).".find(line.byte_slice(item.to))]
    let chunks = make_chunks(layout, before_cs, keyword, after_cs)

    out += [render(layout, chunks, reference)]
  }

  let text = if out.len() > 0 { out.join("\n") + "\n" } else { "" }

  if output == "-" {
    gnu.write_text(text)
    return
  }

  if let Err(failure) = fp"{output}".write(text) {
    gnu.error(f"{gnu.quote(output)}: {gnu.strerror(failure)}")
    exit 1
  }
}
