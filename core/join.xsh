#!/bin/xsh
use lib.gnu
use lib.textio_a1 as tio

const USAGE = """Usage: join [OPTION]... FILE1 FILE2
For each pair of input lines with identical join fields, write a line to
standard output.  The default join field is the first, delimited by blanks.

When FILE1 or FILE2 (not both) is -, read standard input.

  -a FILENUM        also print unpairable lines from file FILENUM, where
                      FILENUM is 1 or 2, corresponding to FILE1 or FILE2
  -e EMPTY          replace missing input fields with EMPTY
  -i, --ignore-case  ignore differences in case when comparing fields
  -j FIELD          equivalent to '-1 FIELD -2 FIELD'
  -o FORMAT         obey FORMAT while constructing output line
  -t CHAR           use CHAR as input and output field separator
  -v FILENUM        like -a FILENUM, but suppress joined output lines
  -1 FIELD          join on this FIELD of file 1
  -2 FIELD          join on this FIELD of file 2
  --check-order     check that the input is correctly sorted, even
                      if all input lines are pairable
  --nocheck-order   do not check that the input is correctly sorted
  --header          treat the first line in each file as field headers,
                      print them without trying to pair them
  -z, --zero-terminated     line delimiter is NUL, not newline
      --help        display this help and exit
      --version     output version information and exit

Unless -t CHAR is given, leading blanks separate fields and are ignored,
else fields are separated by CHAR.  Any FIELD is a field number counted
from 1.  FORMAT is one or more comma or blank separated specifications,
each being 'FILENUM.FIELD' or '0'.  Default FORMAT outputs the join field,
the remaining fields from FILE1, the remaining fields from FILE2, all
separated by CHAR.  If FORMAT is the keyword 'auto', then the first
line of each file determines the number of fields output for each file.

Important: FILE1 and FILE2 must be sorted on the join fields.
E.g., use "sort -k 1b,1" if 'join' has no options,
or use "join -t ''" if 'sort' has no options.
Note, comparisons honor the rules specified by 'LC_COLLATE'.
If the input is not sorted and some lines cannot be joined, a
warning message will be given.
"""

type JoinOptions = {
  unpaired: List[Str],
  empty: Str,
  ignore_case: Bool,
  both: List[Str],
  format: List[Str],
  tab: Str?,
  only: List[Str],
  first: List[Str],
  second: List[Str],
  check_order: Bool,
  nocheck_order: Bool,
  header: Bool,
  zero: Bool,
  help: Bool,
  version: Bool,
  files: List[Str],
}

# One output field: file 0 is the join field, otherwise FILE.FIELD (0-based).
type Spec = {file: Int, field: Int}

# How a line is split (`blank`, `line`, or `sep`) and how output is assembled.
type Layout = {mode: Str, sep: Bytes, out_sep: Bytes, specs: List[Spec], filler: Bytes, keys: List[Int]}

# Everything read from one input: the lines, their fields, and the key of each
# line as compared (case folded when -i).
type Input = {name: Str, texts: List[Bytes], fields: List[List[Bytes]], keys: List[Bytes]}

# Order-check state: whether each file already drew a warning, and whether an
# unpairable line has been seen (the default check starts then).
type Flags = {warned: List[Bool], unpairable: Bool}

pure is_blank(value: Int) -> Bool {
  value == 32 or value == 9 or value == 10
}

# -1, 0 or 1 by unsigned byte order, shorter first.
pure order(left: Bytes, right: Bytes) -> Int {
  let found = left.compare(right)

  return 0 when found.equal
  return -1 when found.left < found.right

  1
}

# The fields of one line, following GNU join's `xfields`.
pure split_fields(line: Bytes, layout: Layout) -> List[Bytes] {
  let total = line.len()

  return [] when total == 0
  return [line] when layout.mode == "line"

  if let Ok(text) = line.utf8() {
    if layout.mode == "blank" {
      let words = [bytes.from_text(hit.text) for hit in rx"[^ \t\n]+".find(text)]

      return [] when words.len() == 0
      return words + [b""] when text.ends_with(" ") or text.ends_with("\t") or text.ends_with("\n")

      return words
    }

    if let Ok(mark) = layout.sep.utf8() {
      return [bytes.from_text(part) for part in text.split(mark)]
    }
  }

  var out: List[Bytes] = []

  if layout.mode == "sep" {
    let width = layout.sep.len()
    let lead = layout.sep.byte_at(0) ?? 0
    var start = 0
    var at = 0

    while at < total {
      let hit = line.byte_at(at) == lead and (width == 1 or (at + width <= total and line[at..at + width] == layout.sep))

      if hit {
        out += [line[start..at]]
        at += width
        start = at
      } else {
        at += 1
      }
    }

    out += [line[start..]]

    return out
  }

  var at = 0

  while at < total and is_blank(line.byte_at(at) ?? 0) {
    at += 1
  }

  return [] when at == total

  var more = true

  while more {
    let start = at

    while at < total and ! is_blank(line.byte_at(at) ?? 0) {
      at += 1
    }

    out += [line[start..at]]

    if at == total {
      more = false
    } else {
      while at < total and is_blank(line.byte_at(at) ?? 0) {
        at += 1
      }

      if at == total {
        out += [b""]
        more = false
      }
    }
  }

  out
}

pure fold(text: Bytes) -> Bytes {
  if let Ok(plain) = text.utf8() {
    return bytes.from_text(plain.upper())
  }

  text.lower()
}

proc read_input(name: Str) [fs, process, env, error, io] -> Bytes {
  guard let data = gnu.read_operand(name) else { |failure|
    gnu.name_error(name, failure)
    exit 1
  }

  data
}

# Records of `data` separated by `sep`; a final unterminated record counts.
pure split_records(data: Bytes, sep: Int) -> List[Bytes] {
  if let Ok(text) = data.utf8() {
    var parts = text.split(if sep == 0 { "\0" } else { "\n" })

    if parts.len() > 0 and parts[parts.len() - 1] == "" {
      parts = parts[..parts.len() - 1]
    }

    return [bytes.from_text(part) for part in parts]
  }

  var records: List[Bytes] = []
  var start = 0

  for index in range(data.len()) {
    if data.byte_at(index) == sep {
      records += [data[start..index]]
      start = index + 1
    }
  }

  if start < data.len() {
    records += [data[start..]]
  }

  records
}

pure field_value(fields: List[Bytes], index: Int, filler: Bytes) -> Bytes {
  if index < fields.len() {
    let found = fields[index]

    if found.len() > 0 {
      return found
    }
  }

  filler
}

# The output line for a pair of lines; `blank` marks the side that is absent
# (an unpairable line from the other file).
pure join_line(left: List[Bytes], right: List[Bytes], blank1: Bool, layout: Layout) -> List[Bytes] {
  var parts: List[Bytes] = []

  if layout.specs.len() > 0 {
    for item in layout.specs {
      if item.file == 0 {
        parts += [if blank1 { field_value(right, layout.keys[1], layout.filler) } else { field_value(left, layout.keys[0], layout.filler) }]
      } else if item.file == 1 {
        parts += [field_value(left, item.field, layout.filler)]
      } else {
        parts += [field_value(right, item.field, layout.filler)]
      }
    }
  } else {
    parts += [if blank1 { field_value(right, layout.keys[1], layout.filler) } else { field_value(left, layout.keys[0], layout.filler) }]

    for index in range(left.len()) {
      if index != layout.keys[0] {
        parts += [field_value(left, index, layout.filler)]
      }
    }

    for index in range(right.len()) {
      if index != layout.keys[1] {
        parts += [field_value(right, index, layout.filler)]
      }
    }
  }

  parts
}

# Report a line that sorts before its predecessor: a warning (once per file),
# or fatal with --check-order.
proc disorder(flags: Flags, side: Int, text: Bytes, name: Str, number: Int, checking: Str) [process, env] -> Flags {
  gnu.error(f"{name}:{number}: is not sorted: {text.utf8() ?? "(not valid UTF-8)"}")

  if checking == "always" {
    exit 1
  }

  var warned = flags.warned

  warned[side] = true

  {...flags, warned: warned}
}

proc emit(parts: List[Bytes], layout: Layout, eol: Bytes) [process, env, io] -> Unit {
  var pieces: List[Bytes] = []

  for index in range(parts.len()) {
    if index > 0 {
      pieces += [layout.out_sep]
    }

    pieces += [parts[index]]
  }

  gnu.write_bytes(bytes.concat([@pieces, eol]))
}

# A field number: 1-based digits, a huge value clamps. Returns the 0-based
# index.
proc field_number(text: Str) [process, env] -> Int {
  if ! rx"^[0-9]+$".matches(text) or rx"^0+$".matches(text) {
    gnu.error(f"invalid field number: {gnu.quote(text)}")
    exit 1
  }

  if text.byte_len() > 18 {
    return tio.MAX_COUNT - 1
  }

  (text.parse_int() ?? 1) - 1
}

# A 0-based field index as the 1-based number in messages; the clamp for a
# value past the integer range reads as the largest unsigned number.
pure shown(index: Int) -> Str {
  if index >= tio.MAX_COUNT - 1 { "18446744073709551615" } else { f"{index + 1}" }
}

proc file_number(text: Str) [process, env] -> Int {
  return 0 when text == "1"
  return 1 when text == "2"

  gnu.error(f"invalid field number: {gnu.quote(text)}")
  exit 1
}

proc parse_format(items: List[Str]) [process, env] -> List[Spec] {
  var specs: List[Spec] = []

  for item in items {
    let words = item.replace(",", " ").replace("\t", " ").split(" ")
    var seen = false

    for word in words {
      if word == "" and words.len() > 1 {
        continue
      }

      seen = true

      if word == "0" {
        specs += [{file: 0, field: 0}]
        continue
      }

      if word.starts_with("0") {
        gnu.error(f"invalid field specifier: {gnu.quote(word)}")
        exit 1
      }

      if word == "" or ! (word.starts_with("1") or word.starts_with("2")) {
        gnu.error(f"invalid file number in field spec: {gnu.quote(word)}")
        exit 1
      }

      if word.byte_len() < 2 or word.byte_slice(1, length: 1) != "." {
        gnu.error(f"invalid field specifier: {gnu.quote(word)}")
        exit 1
      }

      specs += [{file: if word.starts_with("1") { 1 } else { 2 }, field: field_number(word.byte_slice(2))}]
    }

    if ! seen {
      gnu.error("invalid file number in field spec: ''")
      exit 1
    }
  }

  specs
}

# `-j1 FIELD` and `-j2 FIELD` are the old spellings of `-1 FIELD` and `-2 FIELD`.
pure modernize(argv: List[Str]) -> List[Str] {
  var out: List[Str] = []
  var options = true

  for item in argv {
    if item == "--" {
      options = false
    }

    out += [if options and item == "-j1" { "-1" } else if options and item == "-j2" { "-2" } else { item }]
  }

  out
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: JoinOptions = cli.applet(
    modernize(argv),
    {
      gnu: {status: 1},
      unpaired: {form: "-a FILENUM", repeated: true},
      empty: {form: "-e EMPTY", default: ""},
      ignore_case: {form: "-i --ignore-case", default: false},
      both: {form: "-j FIELD", repeated: true},
      format: {form: "-o FORMAT", repeated: true},
      tab: {form: "-t CHAR"},
      only: {form: "-v FILENUM", repeated: true},
      first: {form: "-1 FIELD", repeated: true},
      second: {form: "-2 FIELD", repeated: true},
      check_order: {form: "--check-order", default: false, conflicts: ["nocheck_order"]},
      nocheck_order: {form: "--nocheck-order", default: false, conflicts: ["check_order"]},
      header: {form: "--header", default: false},
      zero: {form: "-z --zero-terminated", default: false},
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
    gnu.version("join")
    return
  }

  if opts.files.len() == 0 {
    gnu.missing_operand()
  }

  if opts.files.len() == 1 {
    gnu.missing_operand_after(opts.files[0])
  }

  if opts.files.len() > 2 {
    gnu.extra_operand(opts.files[2])
  }

  var key1 = -1
  var key2 = -1

  for text in opts.both {
    let value = field_number(text)

    if key1 >= 0 and key1 != value {
      gnu.error(f"incompatible join fields {shown(key1)}, {shown(value)}")
      exit 1
    }

    key1 = value
    key2 = value
  }

  for text in opts.first {
    let value = field_number(text)

    if key1 >= 0 and key1 != value {
      gnu.error(f"incompatible join fields {shown(key1)}, {shown(value)}")
      exit 1
    }

    key1 = value
  }

  for text in opts.second {
    let value = field_number(text)

    if key2 >= 0 and key2 != value {
      gnu.error(f"incompatible join fields {shown(key2)}, {shown(value)}")
      exit 1
    }

    key2 = value
  }

  var print1 = false
  var print2 = false

  for text in opts.unpaired + opts.only {
    if file_number(text) == 0 {
      print1 = true
    } else {
      print2 = true
    }
  }

  let pairable = opts.only.len() == 0
  var mode = "blank"
  var sep = b""

  if opts.tab != null {
    let tab = opts.tab ?? ""

    if tab == "" {
      mode = "line"
    } else if tab == "\\0" {
      mode = "sep"
      sep = b"\0"
    } else if tab.count_chars() == 1 {
      mode = "sep"
      sep = bytes.from_text(tab)
    } else {
      gnu.error(f"multi-character tab {gnu.quote(tab)}")
      exit 1
    }
  }

  var autoformat = opts.format.len() > 0
  var items: List[Str] = []

  for text in opts.format {
    if text != "auto" {
      autoformat = false
      items += [text]
    }
  }

  if items.len() > 0 {
    autoformat = false
  }

  if opts.files[0] == "-" and opts.files[1] == "-" {
    gnu.error("both files cannot be standard input")
    exit 1
  }

  let layout: Layout = {
    mode: mode,
    sep: sep,
    out_sep: if mode == "sep" { sep } else if mode == "line" { b"\n" } else { b" " },
    specs: parse_format(items),
    filler: bytes.from_text(opts.empty),
    keys: [if key1 < 0 { 0 } else { key1 }, if key2 < 0 { 0 } else { key2 }],
  }

  let eol_value = if opts.zero { 0 } else { 10 }
  let eol = bytes.from_ints([eol_value])?
  var inputs: List[Input] = []

  for side in [0, 1] {
    let name = opts.files[side]
    let texts = split_records(read_input(name), eol_value)
    let fields = [split_fields(text, layout) for text in texts]
    let keys = [
      field_value(row, layout.keys[side], b"") for row in fields
    ]

    inputs += [{name: name, texts: texts, fields: fields, keys: if opts.ignore_case { [fold(key) for key in keys] } else { keys }}]
  }

  var layout_out = layout

  if autoformat {
    var specs: List[Spec] = [{file: 0, field: 0}]

    for side in [0, 1] {
      if inputs[side].fields.len() > 0 {
        for index in range(inputs[side].fields[0].len()) {
          if index != layout.keys[side] {
            specs += [{file: side + 1, field: index}]
          }
        }
      }
    }

    layout_out = {...layout, specs: specs}
  }

  let checking = if opts.check_order { "always" } else if opts.nocheck_order { "never" } else { "default" }
  let counts = [inputs[0].texts.len(), inputs[1].texts.len()]
  let ks = [inputs[0].keys, inputs[1].keys]
  let ts = [inputs[0].texts, inputs[1].texts]
  let rows = [inputs[0].fields, inputs[1].fields]
  let names = [inputs[0].name, inputs[1].name]
  var flags: Flags = {warned: [false, false], unpairable: false}
  var next: List[Int] = [0, 0]
  var head: List[Int] = [-1, -1]
  var start: List[Int] = [0, 0]

  if opts.header {
    let has1 = counts[0] > 0
    let has2 = counts[1] > 0

    if has1 or has2 {
      emit(
        join_line(
          if has1 { rows[0][0] } else { [] },
          if has2 { rows[1][0] } else { [] },
          ! has1,
          layout_out,
        ),
        layout_out,
        eol,
      )
    }

    start = [if has1 { 1 } else { 0 }, if has2 { 1 } else { 0 }]
    next = start
  }

  # Taking the next line of a file checks it against the one before it; the
  # check starts once an unpairable line has been seen, or always with
  # --check-order.
  for side in [0, 1] {
    let idx = next[side]

    if idx < counts[side] {
      let watch = checking == "always" or (checking == "default" and flags.unpairable)

      if watch and idx > start[side] and ! flags.warned[side] and order(ks[side][idx - 1], ks[side][idx]) > 0 {
        flags = disorder(flags, side, ts[side][idx], names[side], idx + 1, checking)
      }

      head[side] = idx
      next[side] = idx + 1
    }
  }

  while head[0] >= 0 and head[1] >= 0 {
    let step = order(ks[0][head[0]], ks[1][head[1]])

    if step != 0 {
      let side = if step < 0 { 0 } else { 1 }
      let wanted = if side == 0 { print1 } else { print2 }

      if wanted {
        let row = rows[side][head[side]]
        emit(join_line(if side == 0 { row } else { [] }, if side == 1 { row } else { [] }, side == 1, layout_out), layout_out, eol)
      }

      let idx = next[side]

      if idx < counts[side] {
        let watch = checking == "always" or (checking == "default" and flags.unpairable)

        if watch and idx > start[side] and ! flags.warned[side] and order(ks[side][idx - 1], ks[side][idx]) > 0 {
          flags = disorder(flags, side, ts[side][idx], names[side], idx + 1, checking)
        }

        head[side] = idx
        next[side] = idx + 1
      } else {
        head[side] = -1
      }

      # The check for the next line runs before the first unpairable line is
      # noted, as in GNU.
      flags = {...flags, unpairable: true}

      continue
    }

    var groups: List[List[Int]] = [[head[0]], [head[1]]]
    var ahead: List[Int] = [-1, -1]

    for side in [0, 1] {
      var more = true

      while more and next[side] < counts[side] {
        let idx = next[side]
        let watch = checking == "always" or (checking == "default" and flags.unpairable)

        if watch and idx > start[side] and ! flags.warned[side] and order(ks[side][idx - 1], ks[side][idx]) > 0 {
          flags = disorder(flags, side, ts[side][idx], names[side], idx + 1, checking)
        }

        next[side] = idx + 1

        if order(ks[side][idx], ks[side][head[side]]) == 0 {
          groups[side] += [idx]
        } else {
          ahead[side] = idx
          more = false
        }
      }
    }

    if pairable {
      for one in groups[0] {
        for two in groups[1] {
          emit(join_line(rows[0][one], rows[1][two], false, layout_out), layout_out, eol)
        }
      }
    }

    head = ahead
  }

  for side in [0, 1] {
    let wanted = if side == 0 { print1 } else { print2 }

    while head[side] >= 0 {
      if wanted {
        let row = rows[side][head[side]]
        emit(join_line(if side == 0 { row } else { [] }, if side == 1 { row } else { [] }, side == 1, layout_out), layout_out, eol)
      }

      let idx = next[side]

      if idx < counts[side] {
        let watch = checking == "always" or (checking == "default" and flags.unpairable)

        if watch and idx > start[side] and ! flags.warned[side] and order(ks[side][idx - 1], ks[side][idx]) > 0 {
          flags = disorder(flags, side, ts[side][idx], names[side], idx + 1, checking)
        }

        head[side] = idx
        next[side] = idx + 1
      } else {
        head[side] = -1
      }
    }
  }

  if flags.warned[0] or flags.warned[1] {
    gnu.error("input is not in sorted order")
    exit 1
  }
}
