#!/bin/xsh
use lib.gnu
use lib.text_a2 as text

type Options = {width: Str, goal: Str, crown: Bool, tagged: Bool, split: Bool, uniform: Bool, quick: Bool, prefix: Str, skip_prefix: Str?, exact_prefix: Bool, exact_skip: Bool, help: Bool, version: Bool, paths: List[Str]}

# Width limits report a numeric range error; goals report the selected width's
# data-type bound. Both options use the same diagnostic name.
proc size(value: Str, limit: Int, width: Bool) -> Int {
  if ! rx"^[ \t\n\u{b}\u{c}\r]*[+]?[0-9]+$".matches(value) {
    gnu.error(f"invalid width: {gnu.quote_value(value)}")
    exit 1
  }
  if let Ok(number) = value.trim().parse_int() {
    if number <= limit { return number }
  }
  let reason = if width { "Numerical result out of range" } else { "Value too large for defined data type" }
  gnu.error(f"invalid width: {gnu.quote_value(value)}: {reason}")
  exit 1
}

pure indent(line: Bytes) -> Int {
  var at = 0
  while at < line.len() and line.byte_at(at) == 32 { at += 1 }
  at
}

pure words_of(line: Bytes) -> List[Bytes] {
  var start = 0
  collect {
    for at in range(line.len()) {
      let byte = line.byte_at(at) ?? 0
      if byte == 32 or (byte >= 9 and byte <= 13) {
        yield line[start..at] when at > start
        start = at + 1
      }
    }
    yield line[start..] when start < line.len()
  }
}

type Word = {data: Bytes, gap: Int, new_line: Bool}

pure word_parts(line: Bytes) -> List[Word] {
  var at = 0
  var first = true
  collect {
    while at < line.len() {
      let gap_start = at
      while at < line.len() and ((line.byte_at(at) ?? 0) == 32 or ((line.byte_at(at) ?? 0) >= 9 and (line.byte_at(at) ?? 0) <= 13)) { at += 1 }
      let begin = at
      while at < line.len() and (line.byte_at(at) ?? 0) != 32 and ! ((line.byte_at(at) ?? 0) >= 9 and (line.byte_at(at) ?? 0) <= 13) { at += 1 }
      if at > begin { yield {data: line[begin..at], gap: begin - gap_start, new_line: first}; first = false }
    }
  }
}

type Layout = {cost: Int, length: Int, next: Int}

pure sentence(word: Bytes) -> Bool {
  var at = word.len() - 1
  while at >= 0 and (word.byte_at(at) ?? 0) in [34, 39, 41, 93, 125] { at -= 1 }
  (word.byte_at(at) ?? 0) in [46, 33, 63]
}

# The space written before words[index]. Uniform spacing and a new input line use two spaces only
# after a word that ends a sentence in the input (final), not after any sentence-like word.
pure spacing(words: List[Word], index: Int, uniform: Bool) -> Int {
  if uniform or words[index].new_line { if final_word(words, index - 1) { 2 } else { 1 } } else { words[index].gap }
}

pure final_word(words: List[Word], index: Int) -> Bool {
  index + 1 == words.len() or (sentence(words[index].data) and (words[index + 1].new_line or words[index + 1].gap > 1))
}

# GNU fmt lays out a paragraph in buffers of at most this many words and word characters. A
# fuller buffer is cut at a low-cost break and its tail starts the next buffer, so these limits
# change the output of long paragraphs.
const MAX_BUFFER_WORDS = 998
const MAX_BUFFER_CHARS = 5000

# Costs of a break before words[begin]. The previous word only counts inside the buffer that
# starts at block_start, because a buffer's first line has no earlier line to be compared with.
pure break_cost(words: List[Word], begin: Int, block_start: Int) -> Int {
  var cost = 4900
  if begin > block_start {
    let previous = words[begin - 1].data
    let last = previous.byte_at(previous.len() - 1) ?? 0
    if sentence(previous) { cost += if final_word(words, begin - 1) { -2500 } else { 360000 } } else if (last >= 33 and last <= 47) or (last >= 58 and last <= 64) or (last >= 91 and last <= 96) or (last >= 123 and last <= 126) { cost -= 1600 } else if begin > block_start + 1 and final_word(words, begin - 2) { cost += 40000 / (previous.len() + 2) }
  }
  let first = words[begin].data.byte_at(0) ?? 0
  if first in [34, 39, 40, 91, 96] { cost -= 1600 } else if final_word(words, begin) { cost += 22500 / (words[begin].data.len() + 2) }
  cost
}

type Flushed = {next_start: Int, last_length: Int}

# Costs of the cheapest layout of words[begin..end) for every suffix, indexed by end - start.
# A buffer's first line is measured with first_length and cannot be compared with the line
# before it unless previous_length (the last line already written) is known.
pure layout_buffer(words: List[Word], begin: Int, end: Int, width: Int, goal: Int, first_length: Int, later_length: Int, uniform: Bool, previous_length: Int) -> List[Layout] {
  var layouts: List[Layout] = [{cost: 0, length: 0, next: end}]
  for offset in range(end - begin) {
    let start = end - offset - 1
    var length = if start == begin { first_length } else { later_length }
    var best = {cost: 9223372036854775807, length: 0, next: start + 1}
    for stop in range(start + 1, end + 1) {
      length += words[stop - 1].data.len() + (if stop > start + 1 { spacing(words, stop - 1, uniform) } else { 0 })
      break when length > width and stop > start + 1
      let suffix = layouts[end - stop]
      var cost = suffix.cost
      if stop < end {
        let departure = goal - length
        cost += 100 * departure * departure
        if suffix.next < end { let difference = length - suffix.length; cost += 50 * difference * difference }
      }
      if start == begin and previous_length > 0 { let difference = length - previous_length; cost += 50 * difference * difference }
      if cost < best.cost { best = {cost: cost, length: length, next: stop} }
    }
    layouts += [{cost: best.cost + break_cost(words, start, begin), length: best.length, next: best.next}]
  }
  layouts
}

# Writes words[begin..end) as lines. A flush instead cuts at the line start that GNU's
# credit-biased comparison prefers and writes only the lines before that cut.
proc emit_buffer(words: List[Word], begin: Int, end: Int, flushing: Bool, first: Str, later: Str, uniform: Bool, width: Int, goal: Int, previous_length: Int) -> Flushed {
  let layouts = layout_buffer(words, begin, end, width, goal, first.byte_len(), later.byte_len(), uniform, previous_length)
  var cut = end
  if flushing {
    var best_break = 9223372036854775807
    var candidate = layouts[end - begin].next
    while candidate < end {
      let following = layouts[end - candidate].next
      let line_cost = layouts[end - candidate].cost - layouts[end - following].cost
      if line_cost < best_break { cut = candidate; best_break = line_cost }
      if best_break <= 9223372036854775807 - 3 { best_break += 3 }
      candidate = following
    }
  }
  var line_start = begin
  var last_length = previous_length
  while line_start < cut {
    let line = layouts[end - line_start]
    last_length = line.length
    var out: List[Bytes] = [bytes.from_text(if line_start == begin { first } else { later })]
    for index in range(line_start, line.next) {
      if index > line_start { out += [bytes.from_text(text.padding(spacing(words, index, uniform)))] }
      out += [words[index].data]
    }
    gnu.write_bytes(bytes.concat([@out, b"\n"]))
    line_start = line.next
  }
  {next_start: cut, last_length: last_length}
}

# Optimize each suffix once. Costs favor sentence boundaries, discourage
# false sentence breaks after initials, and balance neighboring filled lines.
proc paragraph(lines: List[Bytes], width: Int, goal: Int, quick: Bool, first: Str, later: Str, uniform: Bool) {
  let words = lines |> flat-map { |line| word_parts(line) }
  if words.is_empty() { return }
  if quick {
    var breaks: List[Int] = []
    var begin = 0
    while begin < words.len() {
      var end = begin
      var length = if begin == 0 { first.byte_len() } else { later.byte_len() }
      while end < words.len() {
        let added = words[end].data.len() + (if end > begin { spacing(words, end, uniform) } else { 0 })
        break when length + added > width and end > begin
        length += added; end += 1
      }
      breaks += [end]; begin = end
    }
    var line_start = 0
    for end in breaks {
      var out: List[Bytes] = [bytes.from_text(if line_start == 0 { first } else { later })]
      for item in words[line_start..end] |> enumerate() {
        if item.index > 0 { out += [bytes.from_text(text.padding(spacing(words, line_start + item.index, uniform)))] }
        out += [item.value.data]
      }
      gnu.write_bytes(bytes.concat([@out, b"\n"])); line_start = end
    }
  } else {
    var begin = 0
    var previous_length = 0
    var chars = 0
    for index in range(words.len()) {
      let size = words[index].data.len()
      if index > begin and (index - begin == MAX_BUFFER_WORDS or chars + size > MAX_BUFFER_CHARS) {
        let flushed = emit_buffer(words, begin, index, true, first, later, uniform, width, goal, previous_length)
        begin = flushed.next_start
        previous_length = flushed.last_length
        chars = 0
        for at in range(begin, index) { chars += words[at].data.len() }
      }
      chars += size
    }
    let _ = emit_buffer(words, begin, words.len(), false, first, later, uniform, width, goal, previous_length)
  }
}

proc modernize(argv: List[Str]) -> List[Str] {
  var enabled = true
  var value = false
  for item in argv |> enumerate() {
    if item.value == "--" { enabled = false }
    if enabled and ! value and rx"^-[0-9]".matches(item.value) {
      if item.index == 0 { return ["-w", item.value.byte_slice(1), @argv[1..]] }
      gnu.error(f"invalid option -- {item.value.byte_slice(1, length: 1)}; -WIDTH is recognized only when it is the first\noption; use -w N instead")
      exit 1
    }
    value = item.value in ["-w", "--width", "-g", "--goal", "-p", "--prefix", "-P", "--skip-prefix"]
  }
  argv
}

proc main(...argv: List[Bytes]) {
  let arguments = text.normalize_arguments(argv)
  let opts: Options = cli.applet(modernize(arguments.values), {
    gnu: {status: 1},
    width: {form: "-w --width WIDTH", default: ""},
    goal: {form: "-g --goal WIDTH", default: ""},
    crown: {form: "-c --crown-margin", default: false},
    tagged: {form: "-t --tagged-paragraph", default: false},
    split: {form: "-s --split-only", default: false},
    uniform: {form: "-u --uniform-spacing", default: false},
    quick: {form: "-q --quick", default: false},
    prefix: {form: "-p --prefix PREFIX", default: ""},
    skip_prefix: {form: "-P --skip-prefix PREFIX"},
    exact_prefix: {form: "-x --exact-prefix", default: false},
    exact_skip: {form: "-X --exact-skip-prefix", default: false},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    paths: {form: "...FILE"},
  })?
  if opts.help { gnu.help("Usage: fmt [OPTION]... [FILE]...\nReformat paragraphs.\n  -w, --width=WIDTH\n  -g, --goal=WIDTH\n  -c, --crown-margin\n  -t, --tagged-paragraph\n  -s, --split-only\n  -u, --uniform-spacing\n  -q, --quick\n  -p, --prefix=PREFIX"); return }
  if opts.version { gnu.version("fmt"); return }
  let requested_width = if opts.width != "" { size(opts.width, 2500, true) } else { -1 }
  let goal_value = if opts.goal == "" { 0 } else { size(opts.goal, if requested_width >= 0 { requested_width } else { 75 }, false) }
  let width = if requested_width >= 0 { requested_width } else if opts.goal != "" { goal_value + 10 } else { 75 }
  # Without -g the goal is LEEWAY (7) percent short of the width, computed as GNU does.
  let goal = if opts.goal == "" { width * 187 / 200 } else { goal_value }
  var failed = false
  let paths = if opts.paths.is_empty() { [b"-"] } else { text.argument_bytes_list(arguments, opts.paths) }
  for name in paths {
    guard let data = text.read_operand_bytes(name) else { |failure| text.cannot_open_bytes(name, failure); failed = true; continue }
    var lines: List[Bytes] = []
    var first = ""
    var later = ""
    for raw in text.records(data) {
      let line = text.expand(raw, {stops: [], interval: 8, relative: false})
      let margin = indent(line)
      let prefix_bytes = bytes.from_text(opts.prefix)
      let prefix_start = if opts.exact_prefix { 0 } else { margin }
      let matches = opts.prefix == "" or line[prefix_start..prefix_start + prefix_bytes.len()] == prefix_bytes
      let skip_start = if opts.exact_skip { 0 } else { margin }
      let skipped = if let prefix = opts.skip_prefix { let encoded = bytes.from_text(prefix); line[skip_start..skip_start + encoded.len()] == encoded } else { false }
      var prefix_end = margin + prefix_bytes.len()
      while prefix_end < line.len() and line.byte_at(prefix_end) == 32 { prefix_end += 1 }
      let current = text.padding(margin) + opts.prefix + text.padding(prefix_end - margin - prefix_bytes.len())
      let blank = words_of(line).is_empty()
      let boundary = blank or skipped or ! matches or opts.split or (opts.tagged and lines.len() == 1 and current == first) or (! lines.is_empty() and current != later and ! ((opts.crown or opts.tagged) and lines.len() == 1))
      if boundary and ! lines.is_empty() {
        paragraph(lines, width, goal, opts.quick, first, later, opts.uniform); lines = []
      }
      if blank or skipped or ! matches { gnu.write_bytes(bytes.concat([raw, b"\n"])); continue }
      if lines.is_empty() { first = current; later = current } else if lines.len() == 1 and (opts.crown or opts.tagged) { later = current }
      lines += [line[current.byte_len()..]]
    }
    paragraph(lines, width, goal, opts.quick, first, later, opts.uniform)
  }
  exit text.finish(failed)
}
