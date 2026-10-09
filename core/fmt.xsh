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

pure wrap_words(words: List[Bytes], width: Int, prefix: Bytes) -> Bytes {
  if words.len() == 0 { return b"\n" }
  let limit = if width > 0 { width } else { 1 }
  var out: List[Bytes] = []
  var line: List[Bytes] = []
  var size = prefix.len()
  for word in words {
    let needed = word.len() + (if line.len() > 0 { 1 } else { 0 })
    if line.len() > 0 and size + needed > limit {
      out += [prefix, bytes.concat(line), b"\n"]
      line = []
      size = prefix.len()
    }
    if line.len() > 0 { line += [b" "] }
    line += [word]
    size += needed
  }
  if line.len() > 0 { out += [prefix, bytes.concat(line), b"\n"] }
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
  skip: Bytes, exact_skip: Bool) -> Bytes {
  let row_list = lines(data)
  var out: List[Bytes] = []
  var paragraph: List[Bytes] = []
  var marked_prefix = b""
  for row in row_list {
    if split_words(row).len() == 0 {
      if paragraph.len() > 0 {
        out += [wrap_words(paragraph, width, marked_prefix)]
        paragraph = []
        marked_prefix = b""
      }
      out += [b"\n"]
    } else if has_prefix(row, skip, exact_skip) {
      if paragraph.len() > 0 { out += [wrap_words(paragraph, width, marked_prefix)]; paragraph = [] }
      out += [row, b"\n"]
    } else {
      var body = row
      if has_prefix(row, prefix, exact_prefix) {
        let offset = if exact_prefix { 0 } else {
          var p = 0
          while p < row.len() and (row.byte_at(p) ?? -1) in [9, 32] { p += 1 }
          p
        }
        body = row[offset + prefix.len()..]
        marked_prefix = bytes.concat([row[..offset], prefix])
      }
      if split {
        if paragraph.len() > 0 { out += [wrap_words(paragraph, width, marked_prefix)]; paragraph = [] }
        out += [wrap_words(split_words(body), width, marked_prefix)]
        marked_prefix = b""
      } else {
        paragraph += split_words(body)
      }
    }
  }
  if paragraph.len() > 0 { out += [wrap_words(paragraph, width, marked_prefix)] }
  bytes.concat(out)
}

pure raw_files(argv: List[Str], raw: List[Bytes]) -> List[Bytes] {
  var files: List[Bytes] = []
  var i = 0
  var stop = false
  while i < argv.len() {
    let arg = argv[i]
    if ! stop and arg == "--" { stop = true; i += 1 } else if ! stop and arg in ["-w", "--width", "-g", "--goal", "-p", "--prefix", "-P", "--skip-prefix"] { i += 2 } else if ! stop and arg.starts_with("--") and arg.find("=") != null { i += 1 } else if ! stop and arg.starts_with("-") and arg != "-" { i += 1 } else { files += [raw[i]]; i += 1 }
  }
  files
}

proc read_input(name: Bytes) [fs, error, io] -> Result[Bytes, Error] {
  return io.stdin_bytes() when name == b"-"
  Path.parse_bytes(name)?.read_bytes()
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
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
  let width_text = final(opts.width, if opts.goal.len() > 0 { f"{goal_guess + 10}" } else { "75" })
  let width = match width_text.parse_int() {
    Ok(value) => value
    Err(_) => { gnu.error(f"invalid width: {gnu.quote_value(width_text)}"); exit 1 }
  }
  if width < 0 or width > 2500 {
    let suffix = if width == 2501 { ": Numerical result out of range" } else { "" }
    gnu.error(f"invalid width: {gnu.quote_value(width_text)}{suffix}")
    exit 1
  }
  let goal = match goal_text.parse_int() {
    Ok(value) => value
    Err(_) => { gnu.error(f"invalid goal: {gnu.quote_value(goal_text)}"); exit 1 }
  }
  if opts.goal.len() > 0 and opts.width.len() > 0 and goal > width { gnu.error("GOAL cannot be greater than WIDTH."); exit 1 }
  if opts.crown { gnu.error("crown-margin is not supported"); exit 1 }
  let names = raw_files(argv, cli.argv_bytes())
  var failed = false
  for name in if names.len() == 0 { [b"-"] } else { names } {
    guard let data = read_input(name) else { |failure|
      if gnu.errno(failure) == 21 { gnu.error(f"{gnu.quote_bytes(name, always: false)}: Is a directory") } else { gnu.cannot_open(gnu.quote_bytes(name, always: false), failure) }
      failed = true
      continue
    }
    gnu.write_bytes(reflow(data, width, opts.split, bytes.from_text(final(opts.prefix, "")), opts.exact_prefix,
      bytes.from_text(final(opts.skip, "")), opts.exact_skip))
  }
  if failed { exit 1 }
}
