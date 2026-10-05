#!/bin/xsh
use lib.gnu
use lib.textio_a1 as tio

const USAGE = """Usage: uniq [OPTION]... [INPUT [OUTPUT]]
Filter adjacent matching lines from INPUT (or standard input),
writing to OUTPUT (or standard output).

With no options, matching lines are merged to the first occurrence.

Mandatory arguments to long options are mandatory for short options too.
  -c, --count           prefix lines by the number of occurrences
  -d, --repeated        only print duplicate lines, one for each group
  -D                    print all duplicate lines
      --all-repeated[=METHOD]  like -D, but allow separating groups
                                 with an empty line;
                                 METHOD={none(default),prepend,separate}
  -f, --skip-fields=N   avoid comparing the first N fields
      --group[=METHOD]  show all items, separating groups with an empty line;
                          METHOD={separate(default),prepend,append,both}
  -i, --ignore-case     ignore differences in case when comparing
  -s, --skip-chars=N    avoid comparing the first N characters
  -u, --unique          only print unique lines
  -z, --zero-terminated     line delimiter is NUL, not newline
  -w, --check-chars=N   compare no more than N characters in lines
      --help        display this help and exit
      --version     output version information and exit

A field is a run of blanks (usually spaces and/or TABs), then non-blank
characters.  Fields are skipped before chars.

Note: 'uniq' does not detect repeated lines unless they are adjacent.
You may want to sort the input first, or use 'sort -u' without 'uniq'.
"""

type UniqOptions = {
  count: Bool,
  repeated: Bool,
  all_dups: Bool,
  all_repeated: Str,
  skip_fields: Str?,
  group: Str,
  ignore_case: Bool,
  skip_chars: Str?,
  unique: Bool,
  zero: Bool,
  check_chars: Str?,
  help: Bool,
  version: Bool,
  files: List[Str],
}

# How lines are compared: fields and characters skipped, at most `width`
# characters compared (-1 for the whole rest), case folded or not.
type Key = {fields: Int, skip: Int, width: Int, fold: Bool, utf8: Bool}

# Which groups print (`unique` singletons, `first` whether a repeated group
# prints its last line too, `later` every repeated group line by line), and how
# groups are delimited.
type Selection = {unique: Bool, first: Bool, later: Bool, delimit: Str, group: Bool, counts: Bool}

# The ARGMATCH abbreviation rules: an exact name or a unique prefix.
pure match_method(value: Str, names: List[Str]) -> List[Str] {
  return [value] when value in names

  [name for name in names if name.starts_with(value)]
}

proc method_or_die(option: Str, value: Str, names: List[Str]) [process, env] -> Str {
  let found = match_method(value, names)

  if found.len() != 1 {
    let list = [f"  - {gnu.quote(name)}" for name in names].join("\n")
    let word = if found.is_empty() { "invalid" } else { "ambiguous" }

    gnu.usage_error(f"{word} argument {gnu.quote(value)} for {gnu.quote(option)}\nValid arguments are:\n{list}")
  }

  found[0]
}

# A count for -f, -s or -w: digits only; a huge value clamps.
proc size_or_die(text: Str, what: Str) [process, env] -> Int {
  let digits = if text.starts_with("+") { text.byte_slice(1) } else { text }

  if digits == "" or ! rx"^[0-9]+$".matches(digits) {
    gnu.error(f"{text}: {what}")
    exit 1
  }

  return tio.MAX_COUNT when digits.byte_len() > 18

  digits.parse_int() ?? 0
}

# `+N` is the obsolete spelling of `-s N` when POSIX 1992 behavior is selected.
proc modernize(argv: List[Str]) [env] -> List[Str] {
  let version = (env.get_or("_POSIX2_VERSION", "") ?? "").parse_int() ?? 200809

  return argv when version >= 200112

  var options = true
  var value = false

  let out: List[Str] = collect {
    for item in argv {
      if item == "--" {
        options = false
      }

      if options and ! value and rx"^\+[0-9]+$".matches(item) {
        yield @["-s", item.byte_slice(1)]
      } else {
        yield item
      }

      value = options and item in ["-f", "-s", "-w", "--skip-fields", "--skip-chars", "--check-chars"]
    }
  }

  out
}

proc utf8_locale() [env] -> Bool {
  var value = ""

  for name in ["LC_ALL", "LC_CTYPE", "LANG"] {
    let found = env.get_or(name, "") ?? ""

    if found != "" {
      value = found
      break
    }
  }

  let lower = value.lower()
  lower.find("utf-8") != null or lower.find("utf8") != null
}

# Records of `data` separated by `sep`; a final unterminated record counts.
proc split_records(data: Bytes, sep: Int) [error] -> Result[List[Bytes]] {
  if let Ok(text) = data.utf8() {
    let mark = if sep == 0 { "\0" } else { "\n" }
    var parts = text.split(mark)

    if ! parts.is_empty() and parts[-1] == "" {
      parts = parts[..parts.len() - 1]
    }

    return [bytes.from_text(part) for part in parts]
  }

  var start = 0

  let records: List[Bytes] = collect {
    for index in range(data.len()) {
      if data.byte_at(index) == sep {
        yield data[start..index]
        start = index + 1
      }
    }

    yield data[start..] when start < data.len()
  }

  records
}

pure is_blank(value: Int) -> Bool {
  value == 32 or value == 9
}

# The part of a line GNU uniq compares: skip FIELDS blank-then-nonblank runs,
# then SKIP characters, then keep at most WIDTH characters.
proc key_of(line: Bytes, key: Key) [error] -> Bytes {
  var rest = line

  if key.fields > 0 {
    let total = line.len()
    var at = 0
    var left = key.fields

    while left > 0 and at < total {
      while at < total and is_blank(line.byte_at(at) ?? 0) {
        at += 1
      }

      while at < total and ! is_blank(line.byte_at(at) ?? 0) {
        at += 1
      }

      left -= 1
    }

    rest = line[at..]
  }

  if key.skip > 0 or key.width >= 0 {
    var done = false

    if key.utf8 {
      if let Ok(text) = rest.utf8() {
        let tail = if key.skip > 0 { text[key.skip..] } else { text }
        rest = bytes.from_text(if key.width >= 0 { tail[..key.width] } else { tail })
        done = true
      }
    }

    if ! done {
      let start = if key.skip > rest.len() { rest.len() } else { key.skip }
      let tail = rest[start..]
      rest = if key.width >= 0 and key.width < tail.len() { tail[..key.width] } else { tail }
    }
  }

  return rest when ! key.fold

  if key.utf8 {
    if let Ok(text) = rest.utf8() {
      return bytes.from_text(text.lower())
    }
  }

  rest.lower()
}

pure number_prefix(count: Int) -> Str {
  let text = f"{count}"
  let pad = if text.byte_len() < 7 { "       ".byte_slice(0, length: 7 - text.byte_len()) } else { "" }

  f"{pad}{text} "
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: UniqOptions = cli.applet(
    modernize(argv),
    {
      gnu: {status: 1},
      count: {form: "-c --count", default: false},
      repeated: {form: "-d --repeated", default: false},
      all_dups: {form: "-D", default: false, conflicts: ["all_repeated"]},
      all_repeated: {form: "--all-repeated[=METHOD]", default: "", optional_default: "none", conflicts: ["all_dups"]},
      skip_fields: {form: "-f --skip-fields N", numeric: true},
      group: {form: "--group[=METHOD]", default: "", optional_default: "separate"},
      ignore_case: {form: "-i --ignore-case", default: false},
      skip_chars: {form: "-s --skip-chars N"},
      unique: {form: "-u --unique", default: false},
      zero: {form: "-z --zero-terminated", default: false},
      check_chars: {form: "-w --check-chars N"},
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
    gnu.version("uniq")
    return
  }

  if opts.files.len() > 2 {
    gnu.extra_operand(opts.files[2])
  }

  let dups = opts.all_dups or opts.all_repeated != ""
  let grouping = opts.group != ""
  let group_method = if grouping {
    method_or_die("--group", opts.group, ["prepend", "append", "separate", "both"])
  } else {
    ""
  }
  let dup_method = if opts.all_repeated != "" {
    method_or_die("--all-repeated", opts.all_repeated, ["none", "prepend", "separate"])
  } else {
    "none"
  }

  if grouping and (opts.count or opts.repeated or opts.unique or dups) {
    gnu.usage_error("--group is mutually exclusive with -c/-d/-D/-u")
  }

  if dups and opts.count {
    gnu.usage_error("printing all duplicated lines and repeat counts is meaningless")
  }

  let key: Key = Key(fields: if opts.skip_fields == null { 0 } else { size_or_die(opts.skip_fields, "invalid number of fields to skip") }, skip: if opts.skip_chars == null { 0 } else { size_or_die(opts.skip_chars, "invalid number of bytes to skip") }, width: if opts.check_chars == null { -1 } else { size_or_die(opts.check_chars, "invalid number of bytes to compare") }, fold: opts.ignore_case, utf8: utf8_locale())

  let select: Selection = Selection(unique: ! opts.repeated and ! dups, first: ! opts.unique, later: dups, delimit: if grouping { group_method } else { dup_method }, group: grouping, counts: opts.count)

  let input_name = opts.files.get(0) ?? "-"
  let output_name = opts.files.get(1) ?? "-"

  guard let data = gnu.read_operand(input_name) else { |failure|
    gnu.name_error(input_name, failure)
    exit 1
  }

  let sep = if opts.zero { 0 } else { 10 }
  let mark = bytes.from_ints([sep])?
  let records = split_records(data, sep)?
  var out: List[Bytes] = []
  var bunch: List[Bytes] = []
  var group_key = b""
  var printed = 0

  for index in range(records.len() + 1) {
    let line = if index < records.len() { records[index] } else { b"" }
    let same = index < records.len() and ! bunch.is_empty() and key_of(line, key) == group_key

    if same {
      bunch += [line]
      continue
    }

    if ! bunch.is_empty() {
      let size = bunch.len()
      var lines: List[Bytes] = []

      if select.group {
        lines = bunch
      } else if select.later {
        if size > 1 {
          lines = if select.first { bunch } else { bunch[..size - 1] }
        }
      } else if (size == 1 and select.unique) or (size > 1 and select.first) {
        lines = [bunch[0]]
      }

      if ! lines.is_empty() {
        let before = select.delimit == "prepend" or select.delimit == "both" or (select.delimit == "separate" and printed > 0)

        if before {
          out += [mark]
        }

        for item in lines {
          if select.counts {
            out += [bytes.from_text(number_prefix(size))]
          }

          out += [item]
          out += [mark]
        }

        if select.delimit == "append" {
          out += [mark]
        }

        printed += 1
      }
    }

    if index < records.len() {
      bunch = [line]
      group_key = key_of(line, key)
    }
  }

  if select.delimit == "both" and printed > 0 {
    out += [mark]
  }

  let result = bytes.concat(out)

  if output_name == "-" {
    gnu.write_bytes(result)
    return
  }

  if let Err(failure) = fp"{output_name}".write(result) {
    gnu.name_error(output_name, failure)
    exit 1
  }
}
