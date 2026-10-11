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

# A half-open range of byte indexes.
type Span = {s: Int, e: Int}

type Chunks = {head: Bytes, before: Bytes, keyword: Bytes, after: Bytes, tail: Bytes}

type Layout = {format: Str, width: Int, gap: Int, truncation: Str, macro_name: Str, refs: Bool, right: Bool, traditional: Bool, auto: Bool}

const WORD_CLASS = "[A-Za-z]+"
const REGEX_SPECIAL = "\\.+*?()|[]{}^$#&-~"

# GNU's default end of a sentence: punctuation, closing quotes or brackets, and
# then a line end, a tab or two spaces.
const SENTENCE_END = "(?m)[.?!][\\]\"')}]*(?:$|\t|  )[ \t\n]*"

pure is_space(c: Bytes) -> Bool {
  c in [b" ", b"\t", b"\n", b"\r", b"\x0b", b"\x0c"]
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
pure trim_span(cs: List[Bytes], range: Span) -> Span {
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

# The default byte map groups ASCII letters; traditional indexing groups
# non-whitespace bytes. Separators advance one byte, even inside UTF-8 text.
pure word_byte(c: Bytes, traditional: Bool) -> Bool {
  let value = c.byte_at(0) ?? 0
  if traditional { ! is_space(c) } else { (value >= 65 and value <= 90) or (value >= 97 and value <= 122) }
}

pure skip_word(cs: List[Bytes], at: Int, limit: Int, traditional: Bool) -> Int {
  var end = at + 1
  if word_byte(cs[at], traditional) {
    while end < limit and word_byte(cs[end], traditional) {
      end += 1
    }
  }
  end
}

# Advance whole words or single separator bytes until the suffix fits.
pure window_ending(cs: List[Bytes], range: Span, width: Int, traditional: Bool) -> Span {
  let end = trim_span(cs, range).e
  var start = range.s
  while start + width < end {
    start = skip_word(cs, start, end, traditional)
  }
  trim_span(cs, {s: start, e: end})
}

# Keep complete words within the width, preserving the separator adjacent to
# the keyword. The wrapped tail uses a strict bound on its final word.
pure window_starting(cs: List[Bytes], range: Span, width: Int, traditional: Bool, strict = false) -> Span {
  var end = range.s
  var cursor = range.s
  let limit = range.s + width
  while cursor < range.e and (if strict { cursor < limit } else { cursor <= limit }) {
    end = cursor
    cursor = skip_word(cs, cursor, range.e, traditional)
  }
  let fits = if strict { cursor < limit } else { cursor <= limit }
  if fits {
    end = cursor
  }
  {s: range.s, e: trim_span(cs, {s: range.s, e: end}).e}
}

pure text_of(cs: List[Bytes], range: Span) -> Bytes {
  bytes.concat(cs[range.s..range.e])
}

pure span_len(range: Span) -> Int {
  range.e - range.s
}

# Lay the keyword and its surroundings out inside the line width: the
# context fields at each side, plus the head and tail that wrap around.
pure make_chunks(layout: Layout, before_cs: List[Bytes], keyword: Bytes, after_cs: List[Bytes]) -> Chunks {
  let half = layout.width / 2
  let max_before = shorter(shorter(half, layout.gap), if layout.traditional { 0 } else { 2 * layout.truncation.byte_len() })
  let max_after = shorter(half, 2 * layout.truncation.byte_len() + keyword.len() + (if layout.traditional { 1 } else { 0 }))
  let before = window_ending(before_cs, {s: 0, e: before_cs.len()}, max_before, layout.traditional)
  let after = window_starting(after_cs, {s: 0, e: after_cs.len()}, max_after, layout.traditional)
  let max_tail = shorter(shorter(max_before, span_len(before)), layout.gap)
  let tail_start = trim_span(after_cs, {s: after.e, e: after_cs.len()}).s
  var tail = window_starting(after_cs, {s: tail_start, e: after_cs.len()}, max_tail, layout.traditional, strict: true)

  if span_len(tail) > 2 and is_space(after_cs[tail.e - 2]) and ! is_space(after_cs[tail.e - 1]) {
    tail = trim_span(after_cs, {s: tail.s, e: tail.e - 1})
  }

  let after_bytes = text_of(after_cs, after).len()
  let max_head = shorter(shorter(max_after, after_bytes), layout.gap)
  let head = window_ending(before_cs, {s: 0, e: before.s}, max_head, layout.traditional)
  var head_text = text_of(before_cs, head)
  var before_text = text_of(before_cs, before)
  var after_text = text_of(after_cs, after)
  var tail_text = text_of(after_cs, tail)

  if layout.format != "tex" {
    if after.e != after_cs.len() {
      if span_len(tail) == 0 {
        after_text = bytes.concat([after_text, bytes.from_text(layout.truncation)])
      } else if tail.e != after_cs.len() {
        tail_text = bytes.concat([tail_text, bytes.from_text(layout.truncation)])
      }
    }

    if before.s != 0 {
      if span_len(head) == 0 {
        before_text = bytes.concat([bytes.from_text(layout.truncation), before_text])
      } else if head.s != 0 {
        head_text = bytes.concat([bytes.from_text(layout.truncation), head_text])
      }
    }
  }

  {head: head_text, before: before_text, keyword: keyword, after: after_text, tail: tail_text}
}

# Escape only the format's ASCII metacharacters, preserving every context
# byte even when the layout cuts through a UTF-8 sequence.
pure tex_field(data: Bytes) -> Bytes {
  bytes.concat(collect {
    for at in range(data.len()) {
      let c = data[at..at + 1]
      yield match c {
        b"\\" => b"\\backslash{}"
        b"$" => b"\\$"
        b"%" => b"\\%"
        b"#" => b"\\#"
        b"&" => b"\\&"
        b"_" => b"\\_"
        b"}" => b"$\\}$"
        b"{" => b"$\\{$"
        _ => c
      }
    }
  })
}

pure roff_field(data: Bytes) -> Bytes {
  bytes.concat([if data[at..at + 1] == b"\"" { b"\"\"" } else { data[at..at + 1] } for at in range(data.len())])
}

pure join_fields(first: Bytes, second: Bytes) -> Bytes {
  return second when first == b""
  return first when second == b""

  bytes.concat([first, b" ", second])
}

pure render(layout: Layout, chunks: Chunks, reference: Str) -> Bytes {
  if layout.format == "tex" {
    let fields = [chunks.tail, chunks.before, chunks.keyword, chunks.after, chunks.head]
    let base = bytes.concat([bytes.from_text(f"\\{layout.macro_name} "), bytes.concat([bytes.concat([b"{", tex_field(field), b"}"]) for field in fields])])
    return if layout.refs { bytes.concat([base, b"{", tex_field(bytes.from_text(reference)), b"}"]) } else { base }
  }

  if layout.format == "roff" {
    let fields = [chunks.tail, chunks.before, bytes.concat([chunks.keyword, chunks.after]), chunks.head]
    let base = bytes.concat([bytes.from_text(f".{layout.macro_name}"), bytes.concat([bytes.concat([b" \"", roff_field(field), b"\""]) for field in fields])])
    return if layout.refs { bytes.concat([base, b" \"", roff_field(bytes.from_text(reference)), b"\""]) } else { base }
  }

  let left = join_fields(chunks.tail, chunks.before)
  let half = if ! layout.refs and layout.width / 2 < layout.gap { layout.gap } else { layout.width / 2 }
  let right = if chunks.head == b"" { chunks.after } else { bytes.concat([chunks.after, bytes.from_text(spaces(shorter(half, chunks.keyword.len() + chunks.after.len() + chunks.head.len()))), chunks.head]) }
  let left_len = if left == bytes.from_text(layout.truncation) and layout.truncation != "" {
    left.len() - layout.truncation.byte_len()
  } else {
    left.len()
  }
  let pad = if left.len() < half { half - left_len } else { 0 }
  let line = bytes.concat([bytes.from_text(spaces(pad)), left, bytes.from_text(spaces(layout.gap)), chunks.keyword, right])

  return line when ! layout.refs

  if layout.right { bytes.concat([line, bytes.from_text(f" {reference}")]) } else { bytes.concat([bytes.from_text(reference + (if layout.auto { ":" + spaces(shorter(layout.gap, 1)) } else { spaces(layout.gap) })), line]) }
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
    let need = if lead < 128 {
      1
    } else if lead >= 194 and lead <= 223 {
      2
    } else if lead >= 224 and lead <= 239 {
      3
    } else if lead >= 240 and lead <= 244 {
      4
    } else {
      0
    }

    if need > 0 and at + need <= total {
      if let Ok(piece) = data[at..at + need].utf8() {
        out = out + piece
        at += need
        continue
      }
    }

    out = out + "�"
    at += 1
  }

  out
}

# Contexts of one input: their text (newlines read as spaces, so offsets
# survive) and the byte offset at which each starts.
type Pieces = {texts: List[Str], starts: List[Int]}

# One context per line, as `ptx -r` and `ptx -G` read their input.
pure line_pieces(text: Str) -> Pieces {
  var texts: List[Str] = []
  var starts: List[Int] = []
  var at = 0
  let parts = text.split("\n")

  for index in range(parts.len()) {
    let part = parts[index]

    if index < parts.len() - 1 or part != "" {
      texts += [if part.ends_with("\r") { part.byte_slice(0, length: part.byte_len() - 1) } else { part }]
      starts += [at]
    }

    at += part.byte_len() + 1
  }

  {texts: texts, starts: starts}
}

# Contexts ending where `ending` matches, the match included and trailing
# whitespace dropped.
pure sentence_pieces(text: Str, ending: Regex) -> Pieces {
  var texts: List[Str] = []
  var starts: List[Int] = []
  var at = 0

  for hit in ending.find(text) {
    texts += [rx"\s+$".replace(text.byte_slice(at, length: hit.end - at).replace("\n", with: " "), with: "")]
    starts += [at]
    at = hit.end
  }

  if at < text.byte_len() {
    texts += [rx"\s+$".replace(text.byte_slice(at).replace("\n", with: " "), with: "")]
    starts += [at]
  }

  {texts: texts, starts: starts}
}

# The line, counted from 1, that holds byte OFFSET, given the offsets of the
# newlines.
pure line_at(breaks: List[Int], offset: Int) -> Int {
  var low = 0
  var high = breaks.len()

  while low < high {
    let mid = (low + high) / 2

    if breaks[mid] < offset {
      low = mid + 1
    } else {
      high = mid
    }
  }

  low + 1
}

pure split_lines(text: Str) -> List[Str] {
  var parts = text.split("\n")

  if ! parts.is_empty() and parts[-1] == "" {
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

# GNU's Emacs regexp syntax treats braces as literal characters. Preserve
# existing escapes and bracket classes while adapting that rule to the engine.
pure word_regexp(pattern: Str) -> Str {
  var out = ""
  var escaped = false
  var in_class = false
  for hit in rx"(?s).".find(patch_backslash(pattern)) {
    let c = hit.text
    if escaped {
      out = out + c
      escaped = false
      continue
    }
    if c == "\\" {
      out = out + c
      escaped = true
      continue
    }
    if c == "[" { in_class = true }
    if c == "]" { in_class = false }
    out = out + (if ! in_class and c in ["{", "}"] { "\\" + c } else { c })
  }
  out
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
    gnu.error(f"{gnu.quote_maybe(name)}: {gnu.strerror(failure)}")
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
      gnu.usage_error(
        f"invalid argument {gnu.quote(opts.format)} for '--format'\nValid arguments are:\n  - 'roff'\n  - 'tex'",
      )
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

  if opts.sentence != null and opts.sentence != "" {
    let pattern = patch_backslash(opts.sentence)

    match regex.compile(pattern) {
      Ok(found) => {
        sentence = found
      }
      Err(failure) => {
        gnu.error(f"Invalid regular expression (for regexp {gnu.quote(opts.sentence ?? "")})")
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
  } else if inputs.is_empty() {
    inputs = ["-"]
  }

  var only_words: List[Str] = []
  var ignore_words: List[Str] = []
  var break_chars = ""

  if opts.only_file != null {
    only_words = split_lines(read_text(opts.only_file))
  }

  if opts.ignore_file != null {
    ignore_words = split_lines(read_text(opts.ignore_file))
  }

  let word_override = opts.word ?? ""

  if opts.break_file != null {
    break_chars = read_text(opts.break_file)

    if opts.traditional {
      break_chars = break_chars + " \t\n"
    }
  }

  var word_pattern = if word_override != "" {
    word_regexp(word_override)
  } else if opts.break_file != null {
    f"[^{escape_class(break_chars)}]+"
  } else if opts.traditional {
    "[^ \t\n]+"
  } else {
    WORD_CLASS
  }

  var files: List[List[Str]] = []
  var starts: List[List[Int]] = []
  var breaks: List[List[Int]] = []
  var ending: Regex? = sentence

  if ending == null and opts.sentence == null and ! opts.references and ! opts.traditional {
    if let Ok(found) = regex.compile(SENTENCE_END) {
      ending = found
    }
  }

  for name in inputs {
    let text = read_text(name)
    if ending != null and text != "" and ending.matches("") {
      gnu.error(f"error: regular expression has a match of length zero: {gnu.quote(opts.sentence ?? "")}")
      exit 1
    }
    let pieces = if ending != null { sentence_pieces(text, ending ?? rx"x") } else { line_pieces(text) }

    files += [pieces.texts]
    starts += [pieces.starts]
    breaks += [[hit.start for hit in rx"\n".find(text)]]
  }

  let word_regex = regex.compile(word_pattern)
  let context_regex = regex.compile(context_pattern)?

  let found: List[Occurrence] = collect {
    if let Ok(words) = word_regex {
      for file in range(files.len()) {
        for index in range(files[file].len()) {
          let line = files[file][index]
          let first = context_regex.find(line)
          let ref_from = if ! first.is_empty() { first[0].start } else { 0 }
          let ref_to = if ! first.is_empty() { first[0].end } else { 0 }

          for hit in words.find(line) {
            let from = hit.start
            let to = hit.end

            continue when from == to

            continue when opts.references and from == ref_from and to == ref_to

            let word = line.byte_slice(from, length: to - from)

            continue when opts.only_file != null and ! (word in only_words)

            continue when opts.ignore_file != null and word in ignore_words

            yield {key: if opts.ignore_case { word.upper() } else { word }, file: file, line: index, from: from, to: to}
          }
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

    if opts.auto_reference and ! ordered.is_empty() {
      line_width = shorter(width, digits + names + 1)
    }
  }

  let layout: Layout = Layout(
    format:,
    width: line_width,
    gap:,
    truncation: opts.truncation,
    macro_name: opts.macro_name,
    refs: layout_refs,
    right: opts.right_refs,
    traditional: opts.traditional,
    auto: opts.auto_reference,
  )

  let out: List[Bytes] = collect {
    for item in ordered {
      let line = files[item.file][item.line]
      let name = inputs[item.file]
      var reference = ""

      if opts.auto_reference {
        let number = line_at(breaks[item.file], starts[item.file][item.line] + item.from)

        reference = if name == "-" { f":{number}" } else { f"{gnu.quote_maybe(name)}:{number}" }
      } else if opts.references {
        let first = context_regex.find(line)

        reference = if ! first.is_empty() { first[0].text } else { "" }
      }

      let before_text = line.byte_slice(0, length: item.from)
      var trimmed = before_text

      if opts.references and reference != "" {
        while trimmed.starts_with(reference) {
          trimmed = trimmed.byte_slice(reference.byte_len())
        }
      }

      let lead = if opts.references {
        before_text.byte_len() - rx"^\s+".replace(trimmed, with: "").byte_len()
      } else {
        0
      }
      let before_data = bytes.from_text(before_text.byte_slice(lead))
      let before_cs = [before_data[at..at + 1] for at in range(before_data.len())]
      let keyword = bytes.from_text(line.byte_slice(item.from, length: item.to - item.from))
      let after_data = bytes.from_text(line.byte_slice(item.to))
      let after_cs = [after_data[at..at + 1] for at in range(after_data.len())]
      let chunks = make_chunks(layout, before_cs, keyword, after_cs)

      yield render(layout, chunks, reference)
    }
  }

  let text = bytes.concat([bytes.concat([line, b"\n"]) for line in out])

  if output == "-" {
    gnu.write_bytes(text)
    return
  }

  if let Err(failure) = fp"{output}".write(text) {
    gnu.error(f"{gnu.quote(output)}: {gnu.strerror(failure)}")
    exit 1
  }
}
