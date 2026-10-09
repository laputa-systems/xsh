#!/bin/xsh
use lib.gnu
use lib.textio_a1 as tio

const USAGE = """
Usage: fmt [OPTION]... [FILE]...
Reformat each paragraph in FILE(s), writing to standard output.
  -c, --crown-margin   preserve the indentation of the first two lines
  -g, --goal=WIDTH     goal width
  -p, --prefix=STRING  reformat lines beginning with STRING
  -P, --skip-prefix=STRING  preserve lines beginning with STRING
  -q, --quick          quick mode
  -s, --split-only     split long lines, but do not reflow
  -u, --uniform-spacing  one space between words
  -w, --width=WIDTH    maximum line width
  -x, --exact-prefix   require prefixes at column zero
  -X, --exact-skip-prefix  require skip prefixes at column zero
      --help           display this help and exit
      --version        output version information and exit
"""

type FmtOptions = {width: List[Str], goal: List[Str], quick: Bool, split: Bool,
  uniform: Bool, prefix: List[Str], skip: List[Str], exact_prefix: Bool,
  exact_skip: Bool, crown: Bool, files: List[Str], help: Bool, version: Bool}

type FmtWord = {text: Bytes, gap: Int, sentence_start: Bool, ends_punct: Bool, new_line: Bool}
type FmtPath = {breaks: List[Int], demerits: Int, previous_delta: Int?, length: Int, fresh: Bool}

pure final(values: List[Str], fallback: Str) -> Str {
  if values.len() == 0 { fallback } else { values[values.len() - 1] }
}

pure positional_width(argv: List[Str]) -> List[Str] {
  var out: List[Str] = []
  var stopped = false
  var first = true
  var takes_value = false
  for arg in argv {
    if ! stopped and arg == "--" { stopped = true; out += [arg] } else if ! stopped and takes_value { out += [arg]; takes_value = false } else if ! stopped and arg in ["-w", "--width", "-g", "--goal", "-p", "--prefix", "-P", "--skip-prefix"] {
      out += [arg]
      takes_value = true
      first = false
    } else if ! stopped and first and rx"^-[0-9]+".matches(arg) { out += [f"--width={arg[1..]}"]; first = false } else { out += [arg]; if ! arg.starts_with("-") { first = false } }
  }
  out
}

pure split_words(data: Bytes) -> List[Bytes] {
  var words: List[Bytes] = []
  var start = -1
  for at in range(data.len()) {
    let byte = data.byte_at(at) ?? 0
    let blank = byte in [9, 10, 11, 12, 13, 32]
    if blank and start >= 0 { words += [data[start..at]]; start = -1 } else if ! blank and start < 0 { start = at }
  }
  if start >= 0 { words += [data[start..]] }
  words
}

pure word_records(row: Bytes, previous_punct: Bool, has_previous: Bool) -> List[FmtWord] {
  var words: List[FmtWord] = []
  var at = 0
  var prev_punct = previous_punct
  var first = true
  while at < row.len() {
    var gap = 0
    while at < row.len() and (row.byte_at(at) ?? -1) in [9, 32] { gap += 1; at += 1 }
    if at >= row.len() { break }
    let start = at
    while at < row.len() and (row.byte_at(at) ?? -1) not in [9, 32] { at += 1 }
    let text = row[start..at]
    let last = text.byte_at(text.len() - 1) ?? -1
    let ends_punct = last in [33, 46, 63]
    let sentence_start = prev_punct and (if first { has_previous } else { gap > 1 })
    let word_gap = if first and ! has_previous { 0 } else if first { if prev_punct { 2 } else { 1 } } else if prev_punct and gap > 1 { 2 } else { 1 }
    words += [{text: text, gap: word_gap, sentence_start: sentence_start, ends_punct: ends_punct, new_line: first}]
    prev_punct = ends_punct
    first = false
  }
  words
}

pure demerits(delta: Int, stretch: Int, word_len: Int, previous_delta: Int?) -> Int {
  let magnitude = if delta < 0 { -delta } else { delta }
  let stretch_cube = stretch * stretch * stretch
  let bad_length = if stretch > 0 { 200 * magnitude * magnitude * magnitude / stretch_cube } else if magnitude == 0 { 0 } else { 1000000000 }
  let short = if stretch > 1 and word_len < stretch { stretch - word_len } else { 0 }
  let denominator = (stretch - 1) * (stretch - 1) * (stretch - 1)
  let bad_word = if short > 0 { 10 * short * short * short / denominator } else { 0 }
  let change = if previous_delta == null { 0 } else { delta - (previous_delta ?? 0) }
  let change_magnitude = if change < 0 { -change } else { change }
  let bad_delta = if previous_delta == null or stretch == 0 { 0 } else { 600 * change_magnitude * change_magnitude * change_magnitude / (8 * stretch_cube) }
  let base = 1 + bad_length + bad_word + bad_delta
  let limited = if base > 1000000000 { 1000000000 } else { base }
  limited * limited
}

pure score_add(left: Int, right: Int) -> Int {
  let maximum = 1000000000000000000
  if left >= maximum or right >= maximum - left { maximum } else { left + right }
}

pure simple_breaks(words: List[FmtWord], width: Int, prefix_len: Int, uniform: Bool) -> List[Int] {
  var breaks: List[Int] = []
  var length = prefix_len + words[0].text.len()
  var previous_punct = words[0].ends_punct
  var index = 1
  while index < words.len() {
    let word = words[index]
    let gap = if word.new_line or uniform { 0 } else { word.gap }
    let spaces = if uniform or word.new_line { if word.sentence_start or (word.new_line and previous_punct) { 2 } else { 1 } } else { 0 }
    let next_length = length + word.text.len() + gap + spaces
    if next_length > width {
      breaks += [index]
      length = prefix_len + word.text.len()
    } else {
      length = next_length
    }
    previous_punct = word.ends_punct
    index += 1
  }
  breaks
}

pure optimal_breaks(words: List[FmtWord], width: Int, goal: Int, prefix_len: Int,
  uniform: Bool) -> List[Int] {
  let count = words.len()
  if count <= 1 { return [] }
  let stretch = width - goal
  let minlength = if goal <= 10 { 1 } else {
    let larger = if goal > stretch + 1 { goal } else { stretch + 1 }
    larger - stretch
  }
  var active: List[FmtPath] = [{breaks: [], demerits: 0, previous_delta: null,
    length: prefix_len + words[0].text.len(), fresh: false}]
  var index = 1
  while index < count {
    let word = words[index]
    let next = if index + 1 < count { words[index + 1] } else { words[index] }
    let is_last = index + 1 == count
    let sentence_end = is_last or next.sentence_start or (next.new_line and word.ends_punct)
    let next_sentence_final = if is_last { false } else {
      if index + 2 >= count { true } else { words[index + 2].sentence_start or (words[index + 2].new_line and next.ends_punct) }
    }
    let slen = if uniform or word.new_line { if word.sentence_start { 2 } else { 1 } } else { 0 }
    var next_active: List[FmtPath] = []
    var best_before: FmtPath? = null
    var best_after: FmtPath? = null
    for candidate in active {
      if ! candidate.fresh and candidate.length >= minlength {
        let delta = goal - candidate.length
        let cost = demerits(delta, stretch, word.text.len(), candidate.previous_delta)
          + (if sentence_end { 250000000 } else { 0 })
        let total = score_add(candidate.demerits, cost)
        let before = {breaks: candidate.breaks + [index], demerits: total,
          previous_delta: delta, length: prefix_len + word.text.len(), fresh: false}
        if best_before == null or total < (best_before ?? before).demerits { best_before = before }
      }

      let gap_width = if candidate.fresh or word.new_line { 0 } else { word.gap }
      let total_len = candidate.length + word.text.len() + gap_width + slen
      if total_len <= width {
        next_active += [{breaks: candidate.breaks, demerits: candidate.demerits,
          previous_delta: candidate.previous_delta, length: total_len, fresh: false}]
        if total_len >= minlength {
          let delta = goal - total_len
          let cost = if is_last { 0 } else { demerits(delta, stretch, word.text.len(), candidate.previous_delta) }
          let total = score_add(score_add(candidate.demerits, cost), if ! is_last and next_sentence_final { 250000000 } else { 0 })
          let after = {breaks: candidate.breaks + [index + 1], demerits: total,
            previous_delta: if is_last { 0 } else { delta }, length: prefix_len, fresh: true}
          if best_after == null or total < (best_after ?? after).demerits { best_after = after }
        }
      }
    }
    if best_before != null { next_active += [best_before ?? active[0]] }
    if best_after != null { next_active += [best_after ?? active[0]] }
    if next_active.len() == 0 {
      let fallback = active[0]
      next_active = [{breaks: fallback.breaks + [index], demerits: 0,
        previous_delta: 1, length: prefix_len + word.text.len(), fresh: false}]
    }
    active = next_active
    index += 1
  }
  var best = active[0]
  for candidate in active { if candidate.demerits < best.demerits { best = candidate } }
  var output: List[Int] = []
  for boundary in best.breaks { if boundary > 0 and boundary < count { output += [boundary] } }
  output
}

pure wrap_words(words: List[FmtWord], width: Int, goal: Int, prefix: Bytes, uniform: Bool, quick: Bool) -> Bytes {
  if words.len() == 0 { return b"\n" }
  let limit = if width > 0 { width } else { 1 }
  let count = words.len()
  let breaks = if quick { simple_breaks(words, limit, prefix.len(), uniform) } else { optimal_breaks(words, limit, goal, prefix.len(), uniform) }
  var out: List[Bytes] = []
  var line_start = 0
  var break_index = 0
  while break_index <= breaks.len() {
    let line_end = if break_index < breaks.len() { breaks[break_index] } else { count }
    out += [prefix]
    var at = line_start
    while at < line_end {
      if at > line_start {
        let spaces = if uniform or words[at].new_line { if words[at].sentence_start { 2 } else { 1 } } else { words[at].gap }
        out += [if spaces > 1 { b"  " } else { b" " }]
      }
      out += [words[at].text]
      at += 1
    }
    out += [b"\n"]
    line_start = line_end
    break_index += 1
  }
  bytes.concat(out)
}

pure lines(data: Bytes) -> List[Bytes] {
  let ends = tio.line_ends(data, false)
  var rows: List[Bytes] = []
  var start = 0
  for end in ends { rows += [data[start..end - 1]]; start = end }
  if start < data.len() { rows += [data[start..]] }
  rows
}

pure has_prefix(line: Bytes, prefix: Bytes, exact: Bool) -> Bool {
  var body = line
  if ! exact {
    var at = 0
    while at < body.len() and (body.byte_at(at) ?? -1) in [9, 32] { at += 1 }
    body = body[at..]
  }
  prefix.len() > 0 and body.len() >= prefix.len() and body[..prefix.len()] == prefix
}

pure reflow(data: Bytes, width: Int, split: Bool, prefix: Bytes, exact_prefix: Bool,
  skip: Bytes, exact_skip: Bool, goal: Int, uniform: Bool, quick: Bool) -> Bytes {
  let row_list = lines(data)
  var out: List[Bytes] = []
  var paragraph: List[FmtWord] = []
  var previous_punct = false
  var marked_prefix = b""
  for row in row_list {
    if split_words(row).len() == 0 {
      if paragraph.len() > 0 {
        out += [wrap_words(paragraph, width, goal, marked_prefix, uniform, quick)]
        paragraph = []
        previous_punct = false
        marked_prefix = b""
      }
      out += [b"\n"]
    } else if has_prefix(row, skip, exact_skip) {
      if paragraph.len() > 0 { out += [wrap_words(paragraph, width, goal, marked_prefix, uniform, quick)]; paragraph = []; previous_punct = false }
      out += [row, b"\n"]
      marked_prefix = b""
    } else if prefix.len() > 0 and ! has_prefix(row, prefix, exact_prefix) {
      if paragraph.len() > 0 { out += [wrap_words(paragraph, width, goal, marked_prefix, uniform, quick)]; paragraph = []; previous_punct = false }
      out += [row, b"\n"]
      marked_prefix = b""
    } else {
      var body = row
      if has_prefix(row, prefix, exact_prefix) {
        let offset = if exact_prefix { 0 } else {
          var p = 0
          while p < row.len() and (row.byte_at(p) ?? -1) in [9, 32] { p += 1 }
          p
        }
        var prefix_end = offset + prefix.len()
        if prefix_end < row.len() and (row.byte_at(prefix_end) ?? -1) in [9, 32] { prefix_end += 1 }
        body = row[prefix_end..]
        marked_prefix = row[..prefix_end]
      } else {
        var indent = 0
        while indent < row.len() and (row.byte_at(indent) ?? -1) in [9, 32] { indent += 1 }
        if paragraph.len() == 0 { marked_prefix = row[..indent] }
        body = row[indent..]
      }
      if split {
        if paragraph.len() > 0 { out += [wrap_words(paragraph, width, goal, marked_prefix, uniform, quick)]; paragraph = []; previous_punct = false }
        out += [wrap_words(word_records(body, false, false), width, goal, marked_prefix, uniform, quick)]
        marked_prefix = b""
      } else {
        let additions = word_records(body, previous_punct, paragraph.len() > 0)
        paragraph += additions
        if additions.len() > 0 { previous_punct = additions[additions.len() - 1].ends_punct }
      }
    }
  }
  if paragraph.len() > 0 { out += [wrap_words(paragraph, width, goal, marked_prefix, uniform, quick)] }
  bytes.concat(out)
}

pure raw_files(argv: List[Str], raw: List[Bytes]) -> List[Bytes] {
  var files: List[Bytes] = []
  var i = 0
  var stop = false
  while i < argv.len() {
    let arg = argv[i]
    if ! stop and arg == "--" { stop = true; i += 1 } else if ! stop and arg in ["-w", "--width", "-g", "--goal", "-p", "--prefix", "-P", "--skip-prefix", "--pref", "--skip-pref"] { i += 2 } else if ! stop and arg.starts_with("--") and arg.find("=") != null { i += 1 } else if ! stop and arg.starts_with("-") and arg != "-" { i += 1 } else { files += [raw[i]]; i += 1 }
  }
  files
}

proc read_input(name: Bytes) [fs, error, io] -> Result[Bytes, Error] {
  return io.stdin_bytes() when name == b"-"
  Path.parse_bytes(name)?.read_bytes()
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  var arg_index = 0
  for arg in argv {
    if arg_index > 0 and rx"^-[0-9]".matches(arg) {
      let digit = arg.byte_slice(1, length: 1)
      gnu.error(f"invalid option -- {digit}; -WIDTH is recognized only when it is the first\noption; use -w N instead")
      exit 1
    }
    arg_index += 1
  }
  let opts: FmtOptions = cli.applet(
    positional_width(argv),
    {
      gnu: {status: 1},
      width: {form: "-w --width WIDTH", default: [], repeated: true},
      goal: {form: "-g --goal WIDTH", default: [], repeated: true},
      quick: {form: "-q --quick", default: false},
      split: {form: "-s --split-only", default: false},
      uniform: {form: "-u --uniform-spacing", default: false},
      prefix: {form: "-p --prefix STRING", default: [], repeated: true},
      skip: {form: "-P --skip-prefix STRING", default: [], repeated: true},
      exact_prefix: {form: "-x --exact-prefix", default: false},
      exact_skip: {form: "-X --exact-skip-prefix", default: false},
      crown: {form: "-c --crown-margin", default: false},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      files: {form: "...FILE"},
    },
  )?
  if opts.help { gnu.help(USAGE); return }
  if opts.version { gnu.version("fmt"); return }
  let goal_text = final(opts.goal, "70")
  let goal_guess = match goal_text.parse_int() {
    Ok(value) => value
    Err(_) => 70
  }
  let width_text = final(opts.width, if opts.goal.len() > 0 and goal_guess + 10 < 75 { f"{goal_guess + 10}" } else { "75" })
  let width = match width_text.parse_int() {
    Ok(value) => value
    Err(_) => { gnu.error(f"invalid width: {gnu.quote_value(width_text)}"); exit 1 }
  }
  if width < 0 or width > 2500 {
    let suffix = if width == 2501 { ": Numerical result out of range" } else { "" }
    gnu.error(f"invalid width: {gnu.quote_value(width_text)}{suffix}")
    exit 1
  }
  let parsed_goal = match goal_text.parse_int() {
    Ok(value) => value
    Err(_) => { gnu.error(f"invalid goal: {gnu.quote_value(goal_text)}"); exit 1 }
  }
  let goal = if opts.goal.len() > 0 { parsed_goal } else if opts.width.len() > 0 { if width == 0 { 0 } else if width * 93 / 100 > 0 { width * 93 / 100 } else { 1 } } else { parsed_goal }
  if opts.goal.len() > 0 and goal > width { gnu.error("GOAL cannot be greater than WIDTH."); exit 1 }
  if opts.crown { gnu.error("crown-margin is not supported"); exit 1 }
  let names = raw_files(argv, cli.argv_bytes())
  var failed = false
  for name in if names.len() == 0 { [b"-"] } else { names } {
    if name != b"-" {
      let target_path = Path.parse_bytes(name)?
      if let Ok(meta) = fs.stat(target_path, follow_symlinks: true) {
        if meta.kind == "dir" {
          gnu.error(f"{gnu.quote_bytes(name, always: false)}: Is a directory")
          failed = true
          continue
        }
      }
    }
    guard let data = read_input(name) else { |failure|
      if gnu.errno(failure) == 21 { gnu.error(f"{gnu.quote_bytes(name, always: false)}: Is a directory") } else { gnu.cannot_open(gnu.quote_bytes(name, always: false), failure) }
      failed = true
      continue
    }
    gnu.write_bytes(reflow(data, width, opts.split, bytes.from_text(final(opts.prefix, "")), opts.exact_prefix,
      bytes.from_text(final(opts.skip, "")), opts.exact_skip, goal, opts.uniform, opts.quick))
  }
  if failed { exit 1 }
}
