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

type Pulled = {idx: Int, flags: Flags}

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

# Take line IDX of file SIDE (or -1 past the end) and check its order against
# the line before it. A violation is a warning, or fatal with --check-order.
proc pull(inputs: List[Input], flags: Flags, side: Int, idx: Int, start: Int, checking: Str) [process, env, io] -> Pulled {
  return {idx: -1, flags: flags} when idx >= inputs[side].texts.len()

  let watch = checking == "always" or (checking == "default" and flags.unpairable)

  if watch and idx > start and ! flags.warned[side] and order(inputs[side].keys[idx - 1], inputs[side].keys[idx]) > 0 {
    let shown = inputs[side].texts[idx].utf8() ?? "(not valid UTF-8)"
    gnu.error(f"{inputs[side].name}:{idx + 1}: is not sorted: {shown}")

    if checking == "always" {
      exit 1
    }

    var warned = flags.warned
    warned[side] = true

    return {idx: idx, flags: {...flags, warned: warned}}
  }

  {idx: idx, flags: flags}
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

  gnu.error(f"invalid file number: {gnu.quote(text)}")
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

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: JoinOptions = cli.applet(
    argv,
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
  var flags: Flags = {warned: [false, false], unpairable: false}
  var next: List[Int] = [0, 0]
  var head: List[Int] = [-1, -1]
  var start: List[Int] = [0, 0]

  if opts.header {
    let has1 = inputs[0].texts.len() > 0
    let has2 = inputs[1].texts.len() > 0

    if has1 or has2 {
      emit(
        join_line(
          if has1 { inputs[0].fields[0] } else { [] },
          if has2 { inputs[1].fields[0] } else { [] },
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

  for side in [0, 1] {
    let got = pull(inputs, flags, side, next[side], start[side], checking)

    flags = got.flags
    head[side] = got.idx

    if got.idx >= 0 {
      next[side] = got.idx + 1
    }
  }

  while head[0] >= 0 and head[1] >= 0 {
    let step = order(inputs[0].keys[head[0]], inputs[1].keys[head[1]])

    if step != 0 {
      let side = if step < 0 { 0 } else { 1 }
      let wanted = if side == 0 { print1 } else { print2 }

      if wanted {
        let row = inputs[side].fields[head[side]]
        emit(join_line(if side == 0 { row } else { [] }, if side == 1 { row } else { [] }, side == 1, layout_out), layout_out, eol)
      }

      flags = {...flags, unpairable: true}

      let got = pull(inputs, flags, side, next[side], start[side], checking)

      flags = got.flags
      head[side] = got.idx

      if got.idx >= 0 {
        next[side] = got.idx + 1
      }

      continue
    }

    var groups: List[List[Int]] = [[head[0]], [head[1]]]
    var ahead: List[Int] = [-1, -1]

    for side in [0, 1] {
      loop {
        let got = pull(inputs, flags, side, next[side], start[side], checking)

        flags = got.flags
        break when got.idx < 0

        next[side] = got.idx + 1

        if order(inputs[side].keys[got.idx], inputs[side].keys[head[side]]) == 0 {
          groups[side] += [got.idx]
        } else {
          ahead[side] = got.idx
          break
        }
      }
    }

    if pairable {
      for one in groups[0] {
        for two in groups[1] {
          emit(join_line(inputs[0].fields[one], inputs[1].fields[two], false, layout_out), layout_out, eol)
        }
      }
    }

    head = ahead
  }

  for side in [0, 1] {
    if head[side] >= 0 {
      flags = {...flags, unpairable: true}
    }

    let wanted = if side == 0 { print1 } else { print2 }

    while head[side] >= 0 {
      if wanted {
        let row = inputs[side].fields[head[side]]
        emit(join_line(if side == 0 { row } else { [] }, if side == 1 { row } else { [] }, side == 1, layout_out), layout_out, eol)
      }

      let got = pull(inputs, flags, side, next[side], start[side], checking)

      flags = got.flags
      head[side] = got.idx

      if got.idx >= 0 {
        next[side] = got.idx + 1
      }
    }
  }

  if flags.warned[0] or flags.warned[1] {
    gnu.error("input is not in sorted order")
    exit 1
  }
}
