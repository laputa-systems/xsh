#!/bin/xsh
use lib.gnu

const USAGE = """Usage: comm [OPTION]... FILE1 FILE2
Compare sorted files FILE1 and FILE2 line by line.

When FILE1 or FILE2 (not both) is -, read standard input.

With no options, produce three-column output.  Column one contains
lines unique to FILE1, column two contains lines unique to FILE2,
and column three contains lines common to both files.

  -1                      suppress column 1 (lines unique to FILE1)
  -2                      suppress column 2 (lines unique to FILE2)
  -3                      suppress column 3 (lines that appear in both files)

      --check-order       check that the input is correctly sorted, even
                            if all input lines are pairable
      --nocheck-order     do not check that the input is correctly sorted
      --output-delimiter=STR  separate columns with STR
      --total             output a summary
  -z, --zero-terminated   line delimiter is NUL, not newline
      --help        display this help and exit
      --version     output version information and exit

Note, comparisons honor the rules specified by 'LC_COLLATE'.

Examples:
  comm -12 file1 file2  Print only lines present in both file1 and file2.
  comm -3  file1 file2  Print lines in file1 not in file2, and vice versa.
"""

type CommOptions = {
  hide1: Bool,
  hide2: Bool,
  hide3: Bool,
  check_order: Bool,
  nocheck_order: Bool,
  delimiter: List[Str],
  total: Bool,
  zero: Bool,
  help: Bool,
  version: Bool,
  files: List[Str],
}

# Records of `data` separated by `sep`; a final unterminated record counts.
proc split_records(data: Bytes, sep: Int) [error] -> Result[List[Bytes]] {
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

# -1, 0 or 1 by unsigned byte order, shorter first.
pure order(left: Bytes, right: Bytes) -> Int {
  let found = left.compare(right)

  return 0 when found.equal
  return -1 when found.left < found.right

  1
}

# Tracks one input's previous line so that a line out of order is reported once.
type Checker = {last: Bytes?, failed: Bool}

proc read_input(name: Str) [fs, process, env, error, io] -> Bytes {
  guard let data = gnu.read_operand(name) else { |failure|
    gnu.name_error(name, failure)
    exit 1
  }

  data
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: CommOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      hide1: {form: "-1", default: false},
      hide2: {form: "-2", default: false},
      hide3: {form: "-3", default: false},
      check_order: {form: "--check-order", default: false, conflicts: ["nocheck_order"]},
      nocheck_order: {form: "--nocheck-order", default: false, conflicts: ["check_order"]},
      delimiter: {form: "--output-delimiter STR", repeated: true},
      total: {form: "--total", default: false},
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
    gnu.version("comm")
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

  var delim = "\t"

  for index in range(opts.delimiter.len()) {
    if opts.delimiter[index] != opts.delimiter[0] {
      gnu.error("multiple output delimiters specified")
      exit 1
    }

    delim = opts.delimiter[0]
  }

  if opts.delimiter.len() > 0 and opts.delimiter[0] == "" {
    delim = "\0"
  }

  let sep = if opts.zero { 0 } else { 10 }
  let mark = bytes.from_ints([sep])?
  let left = split_records(read_input(opts.files[0]), sep)?
  let right = split_records(read_input(opts.files[1]), sep)?

  let lead1 = if opts.hide1 { "" } else { delim }
  let lead2 = if opts.hide2 { "" } else { delim }
  let tab2 = bytes.from_text(lead1)
  let tab3 = bytes.from_text(lead1 + lead2)
  let mode = if opts.check_order { "always" } else if opts.nocheck_order { "never" } else { "differ" }

  var out: List[Bytes] = []
  var first = 0
  var second = 0
  var only1 = 0
  var only2 = 0
  var both = 0
  var differ = false
  var last1: Bytes? = null
  var last2: Bytes? = null
  var bad1 = false
  var bad2 = false
  var stop = false

  while (first < left.len() or second < right.len()) and ! stop {
    let step = if first >= left.len() {
      1
    } else if second >= right.len() {
      -1
    } else {
      order(left[first], right[second])
    }

    if step != 0 {
      differ = true
    }

    let check = mode == "always" or (mode == "differ" and differ)
    var disorder1 = false
    var disorder2 = false

    if step <= 0 {
      let line = left[first]

      if mode != "never" {
        if check and last1 != null and order(line, last1) < 0 {
          disorder1 = true
        }

        last1 = line
      }
    }

    if step >= 0 {
      let line = right[second]

      if mode != "never" {
        if check and last2 != null and order(line, last2) < 0 {
          disorder2 = true
        }

        last2 = line
      }
    }

    if disorder1 and ! bad1 {
      gnu.error("file 1 is not in sorted order")
      bad1 = true
    }

    if disorder2 and ! bad2 and ! (step == 0 and disorder1 and mode == "always") {
      gnu.error("file 2 is not in sorted order")
      bad2 = true
    }

    if mode == "always" and (disorder1 or disorder2) {
      stop = true
      continue
    }

    if step < 0 {
      if ! opts.hide1 {
        out += [left[first], mark]
      }

      first += 1
      only1 += 1
    } else if step > 0 {
      if ! opts.hide2 {
        out += [tab2, right[second], mark]
      }

      second += 1
      only2 += 1
    } else {
      if ! opts.hide3 {
        out += [tab3, left[first], mark]
      }

      first += 1
      second += 1
      both += 1
    }
  }

  if opts.total {
    out += [bytes.from_text(f"{only1}{delim}{only2}{delim}{both}{delim}total"), mark]
  }

  gnu.write_bytes(bytes.concat(out))

  if mode != "never" and (bad1 or bad2) {
    if mode == "differ" {
      gnu.error("input is not in sorted order")
    }

    exit 1
  }
}
