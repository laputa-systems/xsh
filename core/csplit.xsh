#!/bin/xsh
use lib.gnu
use lib.textio_a1 as tio

const USAGE = """Usage: csplit [OPTION]... FILE PATTERN...
Output pieces of FILE separated by PATTERN(s) to files 'xx00', 'xx01', ...,
and output byte counts of each piece to standard output.

Mandatory arguments to long options are mandatory for short options too.
  -b, --suffix-format=FORMAT  use sprintf FORMAT instead of %02d
  -f, --prefix=PREFIX        use PREFIX instead of 'xx'
  -k, --keep-files           do not remove output files on errors
      --suppress-matched     suppress the lines matching PATTERN
  -n, --digits=DIGITS        use specified number of digits instead of 2
  -s, -q, --quiet, --silent  do not print counts of output file sizes
  -z, --elide-empty-files    remove empty output files
      --help        display this help and exit
      --version     output version information and exit

Read standard input if FILE is -.  Each PATTERN may be:
  INTEGER            copy up to but not including specified line number
  /REGEXP/[OFFSET]   copy up to but not including a matching line
  %REGEXP%[OFFSET]   skip to, but not including, a matching line
  {INTEGER}          repeat the previous pattern specified number of times
  {*}                repeat the previous pattern as many times as possible

A line OFFSET is a required '+' or '-' followed by a positive integer.
"""

type CsplitOptions = {
  suffix_format: Str?,
  prefix: Str,
  keep: Bool,
  suppress: Bool,
  digits: Str,
  quiet: Bool,
  elide: Bool,
  help: Bool,
  version: Bool,
  operands: List[Str],
}

# One pattern operand: `kind` is line, up or skip; `limit` the line number
# (kind line); `re` and `offset` for the regex kinds; `text` the operand as
# typed, for messages; `repeat` is how many times it runs (-1 for `{*}`).
type Pattern = {kind: Str, limit: Int, re: Str, offset: Int, text: Str, repeat: Int}

# A finished piece of output as a byte range of the input.
type Piece = {from: Int, to: Int}

# A printf-style suffix conversion, split around the one conversion.
type Format = {pre: Str, post: Str, flags: Str, width: Int, precision: Int, conv: Str}

# Where the walk through the input stands: the next unconsumed line, how many
# of the lines from there were already examined (and so cannot match again),
# and the finished pieces.
type Walk = {cur: Int, held: Int, pieces: List[Piece], failure: Str}

pure line_value(text: Str) -> Int {
  if text.byte_len() > 18 { 9223372036854775807 } else { text.parse_int() ?? 0 }
}

# The number a user wrote, clamped; sign handled by the caller.
pure offset_value(text: Str) -> Int {
  let negative = text.starts_with("-")
  let digits = if text.starts_with("-") or text.starts_with("+") { text.byte_slice(1) } else { text }
  let value = line_value(digits)

  if negative { 0 - value } else { value }
}

# The pieces of PATTERN... operands: each pattern with its `{N}` or `{*}`
# repeat folded in. Reports the failure text, or "" when all parse.
proc parse_patterns(operands: List[Str]) [process, env] -> List[Pattern] {
  var out: List[Pattern] = []
  var index = 0

  while index < operands.len() {
    let text = operands[index]
    index += 1

    var repeat = 0

    if index < operands.len() {
      let next = operands[index]

      if next == "{*}" {
        repeat = -1
        index += 1
      } else {
        let counted = rx"^\{([0-9]+)\}$".captures(next)

        if counted.len() > 0 {
          repeat = line_value(counted[1])
          index += 1
        }
      }
    }

    let slashed = rx"^/(.*)/([+-]?[0-9]+)?$".captures(text)
    let percent = rx"^%(.*)%([+-]?[0-9]+)?$".captures(text)

    if slashed.len() > 0 or percent.len() > 0 {
      let parts = if slashed.len() > 0 { slashed } else { percent }

      if let Err(failure) = regex.compile(parts[1]) {
        gnu.error(f"{gnu.quote(text)}: invalid regular expression: {failure.message}")
        exit 1
      }

      out += [{kind: if slashed.len() > 0 { "up" } else { "skip" }, limit: 0, re: parts[1], offset: if parts[2] == "" { 0 } else { offset_value(parts[2]) }, text: text, repeat: repeat}]
    } else if rx"^[0-9]+$".matches(text) {
      out += [{kind: "line", limit: line_value(text), re: "", offset: 0, text: text, repeat: repeat}]
    } else {
      gnu.error(f"{gnu.quote(text)}: invalid pattern")
      exit 1
    }
  }

  var previous = 0

  for item in out {
    if item.kind == "line" {
      if item.limit == 0 {
        gnu.error("0: line number must be greater than zero")
        exit 1
      }

      if item.limit == previous {
        gnu.error(f"warning: line number '{item.limit}' is the same as preceding line number")
      } else if previous > item.limit {
        gnu.error(f"line number '{item.limit}' is smaller than preceding line number, {previous}")
        exit 1
      }

      previous = item.limit
    }
  }

  out
}

# A printf conversion for an unsigned count, as GNU csplit allows in
# `--suffix-format`.
proc parse_format(text: Str) [process, env] -> Format {
  var found = 0
  var pre = ""
  var post = ""
  var flags = ""
  var width = 0
  var precision = -1
  var conv = "d"
  var at = 0
  let total = text.byte_len()

  while at < total {
    let ch = text.byte_slice(at, length: 1)

    if ch != "%" {
      if found == 0 { pre = pre + ch } else { post = post + ch }

      at += 1
      continue
    }

    if at + 1 < total and text.byte_slice(at + 1, length: 1) == "%" {
      if found == 0 { pre = pre + "%" } else { post = post + "%" }

      at += 2
      continue
    }

    found += 1

    if found > 1 {
      gnu.error("too many % conversion specifications in suffix")
      exit 1
    }

    at += 1

    while at < total and text.byte_slice(at, length: 1) in ["-", "+", " ", "#", "0", "'"] {
      flags = flags + text.byte_slice(at, length: 1)
      at += 1
    }

    var digits = ""

    while at < total and text.byte_slice(at, length: 1) in ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"] {
      digits = digits + text.byte_slice(at, length: 1)
      at += 1
    }

    width = if digits == "" { 0 } else { line_value(digits) }

    if at < total and text.byte_slice(at, length: 1) == "." {
      at += 1

      var places = ""

      while at < total and text.byte_slice(at, length: 1) in ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"] {
        places = places + text.byte_slice(at, length: 1)
        at += 1
      }

      precision = if places == "" { 0 } else { line_value(places) }
    }

    if at >= total {
      gnu.error("missing conversion specifier in suffix")
      exit 1
    }

    conv = text.byte_slice(at, length: 1)

    if ! (conv in ["d", "i", "u", "o", "x", "X"]) {
      gnu.error(f"invalid conversion specifier in suffix: %{conv}")
      exit 1
    }

    if (flags.find("#") != null and conv in ["d", "i", "u"]) or ((flags.find("+") != null or flags.find(" ") != null) and conv in ["u", "o", "x", "X"]) {
      gnu.error(f"invalid flags in conversion specification: %{flags}{conv}")
      exit 1
    }

    at += 1
  }

  if found == 0 {
    gnu.error("missing conversion specifier in suffix")
    exit 1
  }

  {pre: pre, post: post, flags: flags, width: width, precision: precision, conv: conv}
}

pure to_base(value: Int, base: Int, upper: Bool) -> Str {
  return "0" when value == 0

  var rest = value
  var out = ""
  let digits = if upper { "0123456789ABCDEF" } else { "0123456789abcdef" }

  while rest > 0 {
    out = digits.byte_slice(rest % base, length: 1) + out
    rest = rest / base
  }

  out
}

pure spaces(count: Int, fill: Str) -> Str {
  var out = ""

  for _ in range(count) {
    out = out + fill
  }

  out
}

# The suffix of piece NUMBER under the conversion.
pure suffix(spec: Format, number: Int) -> Str {
  let base = if spec.conv == "o" { 8 } else if spec.conv in ["x", "X"] { 16 } else { 10 }
  var digits = if spec.precision == 0 and number == 0 { "" } else { to_base(number, base, spec.conv == "X") }

  if spec.precision > digits.byte_len() {
    digits = spaces(spec.precision - digits.byte_len(), "0") + digits
  }

  var sign = ""

  if flags_has(spec, "#") and spec.conv == "o" and ! digits.starts_with("0") {
    digits = "0" + digits
  } else if flags_has(spec, "#") and spec.conv in ["x", "X"] and number != 0 {
    sign = "0" + spec.conv
  } else if spec.conv in ["d", "i"] and flags_has(spec, "+") {
    sign = "+"
  } else if spec.conv in ["d", "i"] and flags_has(spec, " ") {
    sign = " "
  }

  let body = sign + digits
  let missing = if spec.width > body.byte_len() { spec.width - body.byte_len() } else { 0 }
  var shown = body

  if flags_has(spec, "-") {
    shown = body + spaces(missing, " ")
  } else if flags_has(spec, "0") and spec.precision < 0 {
    shown = sign + spaces(missing, "0") + digits
  } else {
    shown = spaces(missing, " ") + body
  }

  spec.pre + shown + spec.post
}

pure flags_has(spec: Format, flag: Str) -> Bool {
  spec.flags.find(flag) != null
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: CsplitOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      suffix_format: {form: "-b --suffix-format FORMAT"},
      prefix: {form: "-f --prefix PREFIX", default: "xx"},
      keep: {form: "-k --keep-files", default: false},
      suppress: {form: "--suppress-matched", default: false},
      digits: {form: "-n --digits DIGITS", default: ""},
      quiet: {form: "-s -q --quiet --silent", default: false},
      elide: {form: "-z --elide-empty-files", default: false},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      operands: {form: "...OPERAND"},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }

  if opts.version {
    gnu.version("csplit")
    return
  }

  if opts.operands.len() == 0 {
    gnu.missing_operand()
  }

  if opts.operands.len() == 1 {
    gnu.missing_operand_after(opts.operands[0])
  }

  var width = 2

  if opts.digits != "" {
    if ! rx"^[0-9]+$".matches(opts.digits) or opts.digits.byte_len() > 9 {
      gnu.error(f"invalid number: {gnu.quote(opts.digits)}")
      exit 1
    }

    width = line_value(opts.digits)
  }

  let spec = parse_format(opts.suffix_format ?? f"%0{width}d")
  let input_name = opts.operands[0]
  let patterns = parse_patterns(opts.operands[1..])

  guard let data = gnu.read_operand(input_name) else { |failure|
    if gnu.errno(failure) == 21 {
      if ! opts.quiet {
        gnu.write_text("0\n")
      }

      gnu.error("read error: Is a directory")
    } else {
      gnu.cannot_open(input_name, failure)
    }

    exit 1
  }

  let ends = tio.line_ends(data, false)
  var spans = ends

  if data.len() > 0 and (spans.len() == 0 or spans[spans.len() - 1] != data.len()) {
    spans += [data.len()]
  }

  let total = spans.len()
  var texts: List[Str] = []

  var needs_match = false

  for item in patterns {
    if item.kind != "line" {
      needs_match = true
    }
  }

  if needs_match {
    for index in range(total) {
      let from = if index == 0 { 0 } else { spans[index - 1] }
      var stop = spans[index]

      if stop > from and data.byte_at(stop - 1) == 10 {
        stop -= 1
      }

      texts += [data[from..stop].utf8() ?? ""]
    }
  }

  var walk: Walk = {cur: 0, held: 0, pieces: [], failure: ""}
  var forever = false
  var ended = false

  for item in patterns {
    if ended or walk.failure != "" {
      break
    }

    let rule = regex.compile(item.re)
    var round = 0
    let limit = if item.repeat < 0 { -1 } else { item.repeat + 1 }

    while (limit < 0 or round < limit) and walk.failure == "" and ! ended {
      round += 1

      let again = if round > 1 { f" on repetition {round - 1}" } else { "" }
      let begin = walk.cur
      var pieces = walk.pieces

      if item.kind == "line" {
        let target = item.limit * round
        let stop = if target - 1 > begin { target - 1 } else { begin }
        let from = if begin == 0 { 0 } else { spans[begin - 1] }

        if stop >= total {
          pieces += [{from: from, to: if total == 0 { 0 } else { spans[total - 1] }}]
          walk = {cur: total, held: 0, pieces: pieces, failure: f"{gnu.quote(item.text)}: line number out of range{again}"}
        } else {
          let upto = if stop == 0 { 0 } else { spans[stop - 1] }
          var next = stop
          var held = 1

          pieces += [{from: from, to: upto}]

          if stop + 1 == target and opts.suppress {
            next = stop + 1
            held = 0
          }

          walk = {cur: next, held: held, pieces: pieces, failure: ""}
        }

        continue
      }

      forever = forever or item.repeat < 0

      let scan = begin + walk.held
      let first = if begin == 0 { 0 } else { spans[begin - 1] }
      var found = -1

      if let Ok(matcher) = rule {
        var probe = scan

        while probe < total and found < 0 {
          if matcher.matches(texts[probe]) {
            found = probe
          }

          probe += 1
        }
      }

      let keep = item.kind == "up"
      let last = if total == 0 { 0 } else { spans[total - 1] }

      if found < 0 {
        if keep {
          pieces += [{from: first, to: last}]
        }

        if item.repeat < 0 {
          ended = true
          walk = {cur: total, held: 0, pieces: pieces, failure: ""}
        } else {
          walk = {cur: total, held: 0, pieces: pieces, failure: f"{gnu.quote(item.text)}: match not found{again}"}
        }

        continue
      }

      let target = found + item.offset

      if item.offset < 0 and found - begin < 0 - item.offset {
        if keep {
          pieces += [{from: first, to: first}]
        }

        walk = {cur: begin, held: 0, pieces: pieces, failure: f"{gnu.quote(item.text)}: line number out of range{again}"}
        continue
      }

      if item.offset > 0 and target > total {
        if keep {
          pieces += [{from: first, to: last}]
        }

        walk = {cur: total, held: 0, pieces: pieces, failure: f"{gnu.quote(item.text)}: line number out of range{again}"}
        continue
      }

      let upto = if target == 0 { 0 } else { spans[target - 1] }
      var next = target
      var held = if item.offset <= 0 { found - target + 1 } else { 0 }

      if opts.suppress {
        next = if target + 1 > total { total } else { target + 1 }
        held = if item.offset <= 0 { found - target } else { 0 }
      }

      if keep {
        pieces += [{from: first, to: upto}]
      }

      walk = {cur: next, held: held, pieces: pieces, failure: ""}
    }
  }

  var pieces = walk.pieces

  if walk.failure == "" {
    if walk.cur < total {
      let from = if walk.cur == 0 { 0 } else { spans[walk.cur - 1] }

      pieces += [{from: from, to: spans[total - 1]}]
    } else if ! forever {
      let at = if total == 0 { 0 } else { spans[total - 1] }

      pieces += [{from: at, to: at}]
    }
  }

  let writing = walk.failure == "" or opts.keep
  var made: List[Str] = []

  for piece in pieces {
    let size = piece.to - piece.from

    if opts.elide and size == 0 {
      continue
    }

    if writing {
      let name = opts.prefix + suffix(spec, made.len())

      if let Err(failure) = fp"{name}".write(data[piece.from..piece.to]) {
        gnu.name_error(name, failure)

        if ! opts.keep {
          for done in made {
            fp"{done}".remove()
          }
        }

        exit 1
      }

      made += [name]
    } else {
      made += [""]
    }

    if ! opts.quiet {
      gnu.write_text(f"{size}\n")
    }
  }

  if walk.failure != "" {
    gnu.error(walk.failure)
    exit 1
  }
}
