#!/bin/xsh
use lib.gnu
use lib.textio_a1 as tio

const USAGE = """Usage: wc [OPTION]... [FILE]...
  or:  wc [OPTION]... --files0-from=F
Print newline, word, and byte counts for each FILE, and a total line if
more than one FILE is specified.  A word is a non-zero-length sequence of
printable characters delimited by white space.

With no FILE, or when FILE is -, read standard input.

The options below may be used to select which counts are printed, always in
the following order: newline, word, character, byte, maximum line length.
  -c, --bytes            print the byte counts
  -m, --chars            print the character counts
  -l, --lines            print the newline counts
      --files0-from=F    read input from the files specified by
                           NUL-terminated names in file F;
                           If F is - then read names from standard input
  -L, --max-line-length  print the maximum display width
  -w, --words            print the word counts
      --total=WHEN       when to print a line with total counts;
                           WHEN can be: auto, always, only, never
      --help        display this help and exit
      --version     output version information and exit
"""

type WcOptions = {
  bytes: Bool,
  chars: Bool,
  lines: Bool,
  max_line_length: Bool,
  words: Bool,
  files0_from: Str?,
  total: Str,
  help: Bool,
  version: Bool,
  files: List[Str],
}

type Counts = {lines: Int, words: Int, chars: Int, bytes: Int, longest: Int}

# Which counts are shown, in output order.
type Shown = {lines: Bool, words: Bool, chars: Bool, bytes: Bool, longest: Bool}

# One input to count: `name` is what messages and the output line show (`-`
# is standard input). A nonempty `issue` is the message that rejects this
# entry of a --files0-from list.
type Input = {name: Str, path: Path, stdin: Bool, issue: Str}

# Text decoded around invalid bytes: `clean` leaves them out, `marked` holds
# one placeholder word character for each.
type Decoded = {clean: Str, marked: Str}

const MEBIBYTE = 1048576

# Characters `unicode-width` gives no width (controls are matched apart) or
# two columns.
const ZERO = "[\\x{300}-\\x{36f}\\x{483}-\\x{489}\\x{591}-\\x{5bd}\\x{5bf}\\x{5c1}\\x{5c2}\\x{5c4}\\x{5c5}\\x{5c7}\\x{610}-\\x{61a}\\x{64b}-\\x{65f}\\x{670}\\x{6d6}-\\x{6dc}\\x{6df}-\\x{6e4}\\x{6e7}\\x{6e8}\\x{6ea}-\\x{6ed}\\x{711}\\x{730}-\\x{74a}\\x{7a6}-\\x{7b0}\\x{900}-\\x{902}\\x{93a}\\x{93c}\\x{941}-\\x{948}\\x{94d}\\x{951}-\\x{957}\\x{962}\\x{963}\\x{e31}\\x{e34}-\\x{e3a}\\x{e47}-\\x{e4e}\\x{200b}-\\x{200f}\\x{2060}-\\x{2064}\\x{20d0}-\\x{20f0}\\x{fe00}-\\x{fe0f}\\x{fe20}-\\x{fe2f}\\x{feff}\\x{1ab0}-\\x{1aff}\\x{1dc0}-\\x{1dff}]"
const WIDE = "[\\x{1100}-\\x{115f}\\x{231a}\\x{231b}\\x{2329}\\x{232a}\\x{23e9}-\\x{23ec}\\x{23f0}\\x{23f3}\\x{25fd}\\x{25fe}\\x{2614}\\x{2615}\\x{2648}-\\x{2653}\\x{267f}\\x{2693}\\x{26a1}\\x{26aa}\\x{26ab}\\x{26bd}\\x{26be}\\x{26c4}\\x{26c5}\\x{26ce}\\x{26d4}\\x{26ea}\\x{26f2}\\x{26f3}\\x{26f5}\\x{26fa}\\x{26fd}\\x{2705}\\x{270a}\\x{270b}\\x{2728}\\x{274c}\\x{274e}\\x{2753}-\\x{2755}\\x{2757}\\x{2795}-\\x{2797}\\x{27b0}\\x{27bf}\\x{2b1b}\\x{2b1c}\\x{2b50}\\x{2b55}\\x{2e80}-\\x{303e}\\x{3041}-\\x{33ff}\\x{3400}-\\x{4dbf}\\x{4e00}-\\x{9fff}\\x{a000}-\\x{a4cf}\\x{a960}-\\x{a97f}\\x{ac00}-\\x{d7a3}\\x{f900}-\\x{faff}\\x{fe10}-\\x{fe19}\\x{fe30}-\\x{fe6f}\\x{ff00}-\\x{ff60}\\x{ffe0}-\\x{ffe6}\\x{1f300}-\\x{1f64f}\\x{1f900}-\\x{1f9ff}\\x{20000}-\\x{3fffd}]"

pure continuation(data: Bytes, at: Int) -> Bool {
  let byte = data.byte_at(at) ?? 0
  byte >= 128 and byte < 192
}

pure within(data: Bytes, at: Int, low: Int, high: Int) -> Bool {
  let byte = data.byte_at(at) ?? 0
  byte >= low and byte <= high
}

# Length of the valid UTF-8 sequence starting at `at`, or 0.
pure sequence_width(data: Bytes, at: Int) -> Int {
  let lead = data.byte_at(at) ?? 0

  return 1 when lead < 128
  return 2 when lead >= 194 and lead <= 223 and continuation(data, at + 1)
  return 3 when lead == 224 and within(data, at + 1, 160, 191) and continuation(data, at + 2)
  return 3 when ((lead >= 225 and lead <= 236) or lead == 238 or lead == 239) and continuation(data, at + 1) and continuation(data, at + 2)
  return 3 when lead == 237 and within(data, at + 1, 128, 159) and continuation(data, at + 2)
  return 4 when lead == 240 and within(data, at + 1, 144, 191) and continuation(data, at + 2) and continuation(data, at + 3)
  return 4 when lead >= 241 and lead <= 243 and continuation(data, at + 1) and continuation(data, at + 2) and continuation(data, at + 3)
  return 4 when lead == 244 and within(data, at + 1, 128, 143) and continuation(data, at + 2) and continuation(data, at + 3)

  0
}

# Decode `data`; each byte that does not start a valid sequence counts as a
# word character and nothing else.
pure decode(data: Bytes) -> Decoded {
  if let Ok(text) = data.utf8() {
    return {clean: text, marked: text}
  }

  var runs: List[Str] = []
  var start = 0
  var at = 0
  let total = data.len()

  while at < total {
    guard (data.byte_at(at) ?? 0) >= 128 else {
      at += 1
      continue
    }

    let width = sequence_width(data, at)

    if width > 0 {
      at += width
    } else {
      runs += [data[start..at].utf8() ?? ""]
      at += 1
      start = at
    }
  }

  runs += [data[start..total].utf8() ?? ""]

  {clean: runs.join(""), marked: runs.join("X")}
}

pure column_width(text: Str) -> Int {
  text.count_chars() - rx"[\x00-\x1f\x7f-\x9f]".find(text).len()
}

# The longest line's display width: tabs advance to the next multiple of 8,
# controls have no width, and `\r` and `\f` end a line like `\n`.
pure longest_line(text: Str) -> Int {
  let flat = text.replace("\r", "\n").replace("\x0c", "\n")
  var best = 0

  if ! rx"[\t\x00-\x08\x0b\x0e-\x1f\x7f-\x9f]".matches(flat) and ! rx"[\x{300}-\x{36f}\x{483}-\x{489}\x{591}-\x{5bd}\x{200b}-\x{200f}\x{1100}-\x{115f}\x{2e80}-\x{ffff}\x{1f300}-\x{1f9ff}\x{20000}-\x{3fffd}]".matches(flat) {
    for line in flat.split("\n") {
      let size = line.count_chars()
      best = if size > best { size } else { best }
    }

    return best
  }

  let zero = regex.compile(ZERO) ?? rx"x"
  let wide = regex.compile(WIDE) ?? rx"x"

  for line in flat.split("\n") {
    var position = 0
    var first = true

    for piece in line.split("\t") {
      let size = column_width(piece) - zero.find(piece).len() + wide.find(piece).len()

      if first {
        position = size
        first = false
      } else {
        position = position - position % 8 + 8 + size
      }
    }

    best = if position > best { position } else { best }
  }

  best
}

pure count_data(data: Bytes, shown: Shown, posix: Bool) -> Counts {
  var counts = {lines: 0, words: 0, chars: 0, bytes: data.len(), longest: 0}

  if shown.lines {
    counts = {...counts, lines: tio.line_ends(data, false).len()}
  }

  if shown.words or shown.chars or shown.longest {
    let text = decode(data)

    if shown.words {
      let words = if posix { rx"[^\t-\r ]+".find(text.marked).len() } else { text.marked.count_words() }
      counts = {...counts, words: words}
    }

    if shown.chars {
      counts = {...counts, chars: text.clean.count_chars()}
    }

    if shown.longest {
      counts = {...counts, longest: longest_line(text.clean)}
    }
  }

  counts
}

pure add_counts(left: Counts, right: Counts) -> Counts {
  {
    lines: left.lines + right.lines,
    words: left.words + right.words,
    chars: left.chars + right.chars,
    bytes: left.bytes + right.bytes,
    longest: if right.longest > left.longest { right.longest } else { left.longest },
  }
}

pure digits(value: Int) -> Int {
  f"{value}".byte_len()
}

pure column(value: Int, width: Int) -> Str {
  let text = f"{value}"

  if text.byte_len() >= width { text } else { tui.left_pad(text, width) }
}

pure line_for(counts: Counts, shown: Shown, width: Int, title: Str) -> Str {
  var cells: List[Str] = []

  if shown.lines {
    cells += [column(counts.lines, width)]
  }

  if shown.words {
    cells += [column(counts.words, width)]
  }

  if shown.chars {
    cells += [column(counts.chars, width)]
  }

  if shown.bytes {
    cells += [column(counts.bytes, width)]
  }

  if shown.longest {
    cells += [column(counts.longest, width)]
  }

  if title != "" {
    cells += [title]
  }

  cells.join(" ") + "\n"
}

pure shown_count(shown: Shown) -> Int {
  (if shown.lines { 1 } else { 0 }) + (if shown.words { 1 } else { 0 }) + (if shown.chars { 1 } else { 0 }) + (if shown.bytes { 1 } else { 0 }) + (if shown.longest { 1 } else { 0 })
}

# The column width GNU derives from the inputs: the digits of the summed sizes
# of regular files, at least 7 when an input is standard input or not a
# regular file, and 1 for a single input with a single count.
proc number_width(inputs: List[Input], shown: Shown) [fs, error] -> Int {
  return 1 when shown_count(shown) == 1 and inputs.len() == 1

  var minimum = 1
  var total = 0

  for input in inputs {
    if input.stdin {
      minimum = 7
    } else if let Ok(entry) = input.path.metadata() {
      if entry.kind == "file" {
        total += entry.size
      } else {
        minimum = 7
      }
    }
  }

  let wide = if total == 0 { 1 } else { digits(total) }

  if wide > minimum { wide } else { minimum }
}

pure total_choice(value: Str) -> Str {
  let names = ["auto", "always", "only", "never"]

  return value when value in names

  let matches = [name for name in names if value != "" and name.starts_with(value)]

  if matches.len() == 1 { matches[0] } else if matches.len() == 0 { "" } else { "?" }
}

proc posix_mode() [env] -> Bool {
  match env.get("POSIXLY_CORRECT") {
    Ok(_) => true
    Err(_) => false
  }
}

# `read_bytes` returns nothing for files whose size the kernel reports as 0
# (procfs); `read_text` reads them to the end, and a file that is not UTF-8 is
# read line by line.
proc read_file(file: Path) [fs, error] -> Result[Bytes, Error] {
  let data = file.read_bytes()?

  return Ok(data) when data.len() > 0

  if let Ok(text) = file.read_text() {
    return Ok(bytes.from_text(text))
  }

  var pieces: List[Bytes] = []

  for line in file.bytes_lines()? {
    pieces += [line, b"\n"]
  }

  Ok(bytes.concat(pieces))
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: WcOptions = cli.applet(
    argv,
    {
      gnu: {status: 1, unsupported: {"--debug": "SIMD diagnostics are not available"}},
      bytes: {form: "-c --bytes", default: false},
      chars: {form: "-m --chars", default: false},
      lines: {form: "-l --lines", default: false},
      max_line_length: {form: "-L --max-line-length", default: false},
      words: {form: "-w --words", default: false},
      files0_from: {form: "--files0-from FILE"},
      total: {form: "--total WHEN", default: "auto"},
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
    gnu.version("wc")
    return
  }

  let mode = total_choice(opts.total)

  if mode == "" or mode == "?" {
    gnu.error(f"{if mode == "?" { "ambiguous" } else { "invalid" }} argument {gnu.quote(opts.total)} for '--total'")
    eprint "Valid arguments are:\n  - 'auto'\n  - 'always'\n  - 'only'\n  - 'never'"
    gnu.try_help()
    exit 1
  }

  let any = opts.bytes or opts.chars or opts.lines or opts.max_line_length or opts.words
  let shown = {
    lines: opts.lines or ! any,
    words: opts.words or ! any,
    chars: opts.chars,
    bytes: opts.bytes or ! any,
    longest: opts.max_line_length,
  }
  var failed = false
  var inputs: List[Input] = []
  var implicit = false
  var streamed_list = false

  if let list = opts.files0_from {
    if opts.files.len() > 0 {
      gnu.error(f"extra operand {gnu.quote(opts.files[0])}")
      eprint "file operands cannot be combined with --files0-from"
      gnu.try_help()
      exit 1
    }

    let from_stdin = list == "-"
    var data = b""

    if from_stdin {
      match io.stdin_bytes() {
        Ok(read) => data = read
        Err(failure) => {
          gnu.error(f"{gnu.quote_maybe("-")}: read error: {gnu.strerror(failure)}")
          exit 1
        }
      }

      streamed_list = tio.standard_file(0) == ""
    } else {
      match fp"{list}".read_bytes() {
        Ok(read) => data = read
        Err(failure) => {
          if gnu.errno(failure) == 21 {
            gnu.error(f"{gnu.quote_maybe(list)}: read error: {gnu.strerror(failure)}")
          } else {
            gnu.cannot_open(list, failure)
          }

          exit 1
        }
      }
    }

    guard let text = data.utf8() else {
      gnu.error("file names that are not valid UTF-8 are not supported")
      exit 1
    }

    var names = text.split("\0")

    if names.len() > 0 and names[names.len() - 1] == "" {
      names = names[..names.len() - 1]
    }

    var index = 0

    for name in names {
      index += 1

      if name == "" {
        inputs += [{name: "", path: p"", stdin: false, issue: f"{gnu.quote_maybe(list)}:{index}: invalid zero-length file name"}]
      } else if name == "-" and from_stdin {
        inputs += [{name: "", path: p"", stdin: false, issue: "when reading file names from standard input, no file name of '-' allowed"}]
      } else {
        inputs += [{name: name, path: fp"{name}", stdin: name == "-", issue: ""}]
      }
    }
  } else if opts.files.len() == 0 {
    implicit = true
    inputs = [{name: "-", path: p"-", stdin: true, issue: ""}]
  } else {
    inputs = [{name: name, path: fp"{name}", stdin: name == "-", issue: ""} for name in opts.files]
  }

  let width = if mode == "only" {
    1
  } else if implicit {
    if shown_count(shown) == 1 { 1 } else { 7 }
  } else if streamed_list {
    1
  } else {
    number_width([item for item in inputs if item.issue == ""], shown)
  }

  let posix = posix_mode()
  var total = {lines: 0, words: 0, chars: 0, bytes: 0, longest: 0}
  var out = ""
  var seen = 0
  let only_bytes = shown.bytes and ! shown.lines and ! shown.words and ! shown.chars and ! shown.longest

  for input in inputs {
    seen += 1

    if input.issue != "" {
      gnu.error(input.issue)
      failed = true
      continue
    }

    var data = b""
    var read_error: Str? = null
    var counted: Counts? = null

    if input.stdin {
      match io.stdin_bytes() {
        Ok(read) => data = read
        Err(failure) => read_error = gnu.strerror(failure)
      }
    } else {
      if only_bytes {
        if let Ok(entry) = input.path.metadata() {
          if entry.kind == "file" and entry.size > MEBIBYTE {
            counted = {lines: 0, words: 0, chars: 0, bytes: entry.size, longest: 0}
          }
        }
      }

      if counted == null {
        match read_file(input.path) {
          Ok(read) => data = read
          Err(failure) => {
            if gnu.errno(failure) == 21 {
              read_error = gnu.strerror(failure)
            } else {
              gnu.name_error(input.name, failure)
              failed = true
              continue
            }
          }
        }
      }
    }

    let counts = counted ?? count_data(data, shown, posix)

    total = add_counts(total, counts)

    if mode != "only" {
      let title = if input.stdin and implicit { "" } else if input.name.find("\n") != null { gnu.quote_bytes(bytes.from_text(input.name), always: false) } else { input.name }
      out += line_for(counts, shown, width, title)
    }

    if let failure = read_error {
      let label = if input.stdin { "standard input" } else { gnu.quote_maybe(input.name) }
      gnu.write_text(out)
      out = ""
      gnu.error(f"{label}: {failure}")
      failed = true
    }
  }

  let show_total = mode == "always" or mode == "only" or (mode == "auto" and seen > 1)

  if show_total {
    out += line_for(total, shown, width, if mode == "only" { "" } else { "total" })
  }

  gnu.write_text(out)

  if failed {
    exit 1
  }
}
