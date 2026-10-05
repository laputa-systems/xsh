#!/bin/xsh
use lib.gnu
use lib.text_a2 as text

type Options = {width: Str, goal: Str, crown: Bool, tagged: Bool, split: Bool, uniform: Bool, quick: Bool, prefix: Str, skip_prefix: Str?, exact_prefix: Bool, exact_skip: Bool, help: Bool, version: Bool, paths: List[Str]}

proc size(value: Str, what: Str) -> Int {
  let number = value.parse_int() ?? -1
  if number < 0 or number > 2500 {
    gnu.error(f"invalid {what}: {gnu.quote_value(value)}{if number > 2500 and number <= 2147483647 { ": Numerical result out of range" } else { "" }}")
    exit 1
  }
  number
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

type Layout = {end: Int, cost: Float, ratio: Float, breaks: List[Int]}

pure sentence(word: Bytes) -> Bool {
  var at = word.len() - 1
  while at >= 0 and (word.byte_at(at) ?? 0) in [34, 39, 41, 93, 125] { at -= 1 }
  (word.byte_at(at) ?? 0) in [46, 33, 63]
}

pure spacing(previous: Word, current: Word, uniform: Bool) -> Int {
  if uniform or current.new_line { if sentence(previous.data) { 2 } else { 1 } } else { current.gap }
}

# Keep the best complete path for each candidate line ending, including the
# change in fullness between adjacent lines and short sentence-final words.
proc paragraph(lines: List[Bytes], width: Int, goal: Int, quick: Bool, first: Str, later: Str, uniform: Bool) {
  let words = lines |> flat-map { |line| word_parts(line) }
  if words.is_empty() { return }
  var breaks: List[Int] = []
  if quick {
    var begin = 0
    while begin < words.len() {
      var end = begin
      var length = if begin == 0 { first.byte_len() } else { later.byte_len() }
      while end < words.len() {
        let added = words[end].data.len() + (if end > begin { spacing(words[end - 1], words[end], uniform) } else { 0 })
        break when length + added > width and end > begin
        length += added; end += 1
      }
      breaks += [end]; begin = end
    }
  } else {
    var paths: List[Layout] = [{end: 0, cost: 0.0, ratio: 0.0, breaks: []}]
    let stretch = if width > goal { width - goal } else { 1 }
    let minimum = if goal <= 10 { 1 } else if goal > stretch { goal - stretch } else { 1 }
    for begin in range(words.len()) {
      let active = paths |> where .end == begin
      if active.is_empty() { continue }
      paths = paths |> where .end != begin
      var length = if begin == 0 { first.byte_len() } else { later.byte_len() }
      for end in range(begin + 1, words.len() + 1) {
        length += words[end - 1].data.len() + (if end > begin + 1 { spacing(words[end - 2], words[end - 1], uniform) } else { 0 })
        break when length > width and end > begin + 1
        if length < minimum and end != words.len() { continue }
        let ratio = (goal - length).float() / stretch.float()
        let word_len = words[end - 1].data.len()
        let short = if word_len < stretch and stretch > 1 { (stretch - word_len).float() / (stretch - 1).float() } else { 0.0 }
        var best = active[0]
        var best_cost = 1.0e100
        for layout in active {
          let delta = if begin == 0 { 0.0 } else { (ratio - layout.ratio) / 2.0 }
          let base = 1.0 + 200.0 * (ratio * ratio * ratio).abs() + 10.0 * (short * short * short).abs() + 600.0 * (delta * delta * delta).abs()
          let orphan = end < words.len() and (end + 1 == words.len() or sentence(words[end].data))
          let penalty = if end == words.len() { 0.0 } else { base * base + (if orphan { 250000000.0 } else { 0.0 }) }
          let cost = layout.cost + penalty
          if cost < best_cost { best_cost = cost; best = layout }
        }
        paths += [{end: end, cost: best_cost, ratio: ratio, breaks: best.breaks + [end]}]
      }
    }
    let finished = paths |> where .end == words.len()
    if finished.is_empty() { gnu.error("unable to lay out paragraph"); exit 1 }
    var best = finished[0]
    for layout in finished { if layout.cost < best.cost { best = layout } }
    breaks = best.breaks
  }
  var begin = 0
  for end in breaks {
    var out: List[Bytes] = [bytes.from_text(if begin == 0 { first } else { later })]
    for item in words[begin..end] |> enumerate() {
      if item.index > 0 { out += [bytes.from_text(text.padding(spacing(words[begin + item.index - 1], item.value, uniform)))] }
      out += [item.value.data]
    }
    gnu.write_bytes(bytes.concat([@out, b"\n"])); begin = end
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

proc main(...argv: List[Str]) {
  let opts: Options = cli.applet(modernize(argv), {
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
  let requested_width = if opts.width != "" { size(opts.width, "width") } else { -1 }
  let goal_value = if opts.goal == "" { 0 } else { size(opts.goal, "goal") }
  let width = if requested_width >= 0 { requested_width } else if opts.goal != "" and goal_value < 65 { goal_value + 10 } else { 75 }
  let goal = if opts.goal == "" { width * 93 / 100 } else { goal_value }
  if goal > width { gnu.error("GOAL cannot be greater than WIDTH."); exit 1 }
  var failed = false
  for name in if opts.paths.is_empty() { ["-"] } else { opts.paths } {
    guard let data = gnu.read_operand(name) else { |failure| gnu.cannot_open(name, failure); failed = true; continue }
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
