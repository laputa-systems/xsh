#!/bin/xsh
use lib.gnu

type SortOptions = {
  reverse: Bool,
  unique: Bool,
  numeric: Bool,
  fold_case: Bool,
  blank: Bool,
  key: Str,
  delimiter: Str,
  output: Str,
  check: Str,
  short_check: Bool,
  silent_check: Bool,
  zero_terminated: Bool,
  paths: List[Str],
}

pure numeric_key(line: Str) -> Int {
  (line.trim().words().get(0) ?? "0").parse_int() ?? 0
}

pure key_index(spec: Str) -> Int {
  (((spec.split(",").get(0) ?? "1").split(".").get(0) ?? "1").parse_int() ?? 1) - 1
}

pure field_key(line: Str, delimiter: Str, field: Int, fold_case: Bool) -> Str {
  let parts = if delimiter == "" { line.trim().words() } else { line.split(delimiter) }
  let key = parts.get(field) ?? ""
  if fold_case {
    key.lower()
  } else {
    key
  }
}

pure numeric_field_key(line: Str, delimiter: Str, field: Int) -> Int {
  field_key(line, delimiter, field, false).parse_int() ?? 0
}

pure trim_leading_blanks(line: Str) -> Str {
  var at = 0
  while at < line.byte_len() and line.byte_slice(at, length: 1) in [" ", "\t"] { at += 1 }
  line.byte_slice(at)
}

pure blank_text_key(line: Str, fold_case: Bool) -> Str {
  let key = trim_leading_blanks(line)
  if fold_case { key.lower() } else { key }
}

## GNU sort uses the full line as a last-resort key after the blank-skipping key.
pure blank_sorted(lines: List[Str], reverse: Bool, fold_case: Bool) -> List[Str] {
  let fallback = if reverse { lines |> sort-by(desc: true) . } else { lines |> sort }
  if reverse {
    fallback |> sort-by(desc: true) blank_text_key(., fold_case)
  } else {
    fallback |> sort-by blank_text_key(., fold_case)
  }
}

pure pair_is_ordered(left: Str, right: Str, opts: SortOptions, has_key: Bool, key_field: Int) -> Bool {
  let pair = [left, right]
  let ordered = if opts.numeric and has_key {
    if opts.reverse { pair |> sort-by(desc: true) numeric_field_key(., opts.delimiter, key_field) } else { pair |> sort-by numeric_field_key(., opts.delimiter, key_field) }
  } else if has_key {
    if opts.reverse { pair |> sort-by(desc: true) field_key(., opts.delimiter, key_field, opts.fold_case) } else { pair |> sort-by field_key(., opts.delimiter, key_field, opts.fold_case) }
  } else if opts.numeric {
    if opts.reverse { pair |> sort-by(desc: true) numeric_key(.) } else { pair |> sort-by numeric_key(.) }
  } else if opts.blank {
    blank_sorted(pair, opts.reverse, opts.fold_case)
  } else if opts.fold_case {
    if opts.reverse { pair |> sort-by(desc: true) .lower() } else { pair |> sort-by .lower() }
  } else if opts.reverse {
    pair |> sort-by(desc: true) .
  } else {
    pair |> sort
  }
  ordered[0] == left
}

pure same_sort_key(left: Str, right: Str, opts: SortOptions, has_key: Bool, key_field: Int) -> Bool {
  if opts.numeric and has_key {
    numeric_field_key(left, opts.delimiter, key_field) == numeric_field_key(right, opts.delimiter, key_field)
  } else if has_key {
    field_key(left, opts.delimiter, key_field, opts.fold_case) == field_key(right, opts.delimiter, key_field, opts.fold_case)
  } else if opts.numeric {
    numeric_key(left) == numeric_key(right)
  } else if opts.blank {
    blank_text_key(left, opts.fold_case) == blank_text_key(right, opts.fold_case)
  } else if opts.fold_case {
    left.lower() == right.lower()
  } else {
    left == right
  }
}

pure input_records(input: Str, zero_terminated: Bool) -> List[Str] {
  if zero_terminated {
    if input == "" { return [] }
    var records = input.split("\0")
    if ! records.is_empty() and records[-1] == "" {
      records = records[..records.len() - 1]
    }
    records
  } else {
    input.lines().collect()
  }
}

## A final unterminated record in one operand ends before the next operand starts.
proc read_sort_input(paths: List[Str], separator: Str) [fs, io, error] -> Result[Str, Error] {
  var sources = paths
  if sources.is_empty() { sources = ["-"] }

  var input = ""
  for source in sources {
    let part = if source == "-" { io.stdin_text()? } else { fp"{source}".read_text()? }
    if input != "" and ! input.ends_with(separator) {
      input += separator
    }
    input += part
  }
  input
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: SortOptions = cli.applet(
    argv,
    {
      reverse: {
        form: "-r --reverse",
        default: false,
      },
      unique: {
        form: "-u --unique",
        default: false,
      },
      numeric: {
        form: "-n --numeric-sort",
        default: false,
      },
      fold_case: {
        form: "-f --ignore-case",
        default: false,
      },
      blank: {
        form: "-b --ignore-leading-blanks",
        default: false,
      },
      key: {
        form: "-k KEY",
        default: "",
      },
      delimiter: {
        form: "-t DELIMITER",
        default: "",
      },
      output: {
        form: "-o FILE",
        default: "",
      },
      check: {
        form: "--check[=TYPE]",
        default: "",
        optional_default: "diagnose-first",
      },
      short_check: {
        form: "-c",
        default: false,
      },
      silent_check: {
        form: "-C",
        default: false,
      },
      zero_terminated: {
        form: "-z --zero-terminated",
        default: false,
      },
      paths: {
        form: "...FILE",
      },
    },
  )?
  let has_key = opts.key != ""
  let key_field = if has_key { key_index(opts.key) } else { 0 }
  let has_output = opts.output != ""
  let output = if has_output { fp"{opts.output}" } else { p"" }
  let check_mode = if opts.check != "" { opts.check } else if opts.short_check { "diagnose-first" } else { "" }
  let check_enabled = check_mode != "" or opts.silent_check
  let silent_check = opts.silent_check or check_mode in ["silent", "quiet", "silen", "quie", "s", "q"]
  let {delimiter, paths, ..} = opts

  let check_is_silent = check_mode in ["silent", "quiet", "silen", "quie", "s", "q"]
  if opts.silent_check and (opts.short_check or (opts.check != "" and ! check_is_silent)) {
    gnu.error("options '-cC' are incompatible")
    exit 2
  }
  if check_enabled and check_mode != "" and check_mode not in ["diagnose-first", "diagnose", "d", "silent", "quiet", "silen", "quie", "s", "q"] {
    gnu.error(f"invalid argument {gnu.quote_maybe(check_mode)} for '--check'")
    exit 2
  }
  if check_enabled and has_output {
    gnu.error(if silent_check { "options '-Co' are incompatible" } else { "options '-co' are incompatible" })
    exit 2
  }

  for input_path in paths {
    if input_path != "-" {
      if let Err(failure) = fs.stat(fp"{input_path}", follow_symlinks: true) {
        gnu.error(f"cannot read: {gnu.quote_maybe(input_path)}: {gnu.strerror(failure)}")
        exit 2
      }
    }
  }

  let input_separator = if opts.zero_terminated { "\0" } else { "\n" }
  let input = read_sort_input(paths, input_separator)?
  let input_lines = input_records(input, opts.zero_terminated)

  if check_enabled {
    if input_lines.len() > 1 {
      for index in range(1, input_lines.len()) {
        let previous = input_lines[index - 1]
        let current = input_lines[index]
        let unordered = ! pair_is_ordered(previous, current, opts, has_key, key_field)
        let duplicate = opts.unique and same_sort_key(previous, current, opts, has_key, key_field)

        if unordered or duplicate {
          if ! silent_check {
            let name = paths.get(0) ?? "-"
            if opts.zero_terminated {
              io.write_stderr(f"{gnu.prog()}: {gnu.quote_maybe(name)}:{index + 1}: disorder: {current}\0")?
            } else {
              gnu.error(f"{gnu.quote_maybe(name)}:{index + 1}: disorder: {current}")
            }
          }
          exit 1
        }
      }
    }
    return
  }

  let sorted = if opts.numeric and has_key {
    if opts.reverse {
      input_lines |> sort-by(desc: true) numeric_field_key(., delimiter, key_field)
    } else {
      input_lines |> sort-by numeric_field_key(., delimiter, key_field)
    }
  } else if has_key {
    if opts.reverse {
      input_lines |> sort-by(desc: true) field_key(., delimiter, key_field, opts.fold_case)
    } else {
      input_lines |> sort-by field_key(., delimiter, key_field, opts.fold_case)
    }
  } else if opts.numeric {
    if opts.reverse {
      input_lines |> sort-by(desc: true) numeric_key(.)
    } else {
      input_lines |> sort-by numeric_key(.)
    }
  } else if opts.blank {
    blank_sorted(input_lines, opts.reverse, opts.fold_case)
  } else if opts.fold_case {
    if opts.reverse {
      input_lines |> sort-by(desc: true) .lower()
    } else {
      input_lines |> sort-by .lower()
    }
  } else if opts.reverse {
    input_lines |> sort-by(desc: true) .
  } else {
    input_lines |> sort
  }

  let lines = if opts.unique { sorted |> unique-by . } else { sorted }

  let line_ending = if opts.zero_terminated { "\0" } else { "\n" }
  let text = if lines.is_empty() {
    ""
  } else {
    f"{lines.join(line_ending)}{line_ending}"
  }

  if has_output {
    output.write(text)
  } else if opts.zero_terminated {
    gnu.write_text(text)
  } else {
    for line in lines {
      print $line
    }
  }
}
