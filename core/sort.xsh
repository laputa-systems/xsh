#!/bin/xsh
use lib.gnu

type SortOptions = {
  reverse: Bool,
  unique: Bool,
  numeric: Bool,
  fold_case: Bool,
  dictionary: Bool,
  ignore_nonprinting: Bool,
  blank: Bool,
  stable: Bool,
  version_sort: Bool,
  sort_mode: List[Str],
  key: Str,
  delimiter: Str,
  output: List[Str],
  check: Str,
  short_check: Bool,
  silent_check: Bool,
  zero_terminated: Bool,
  version: Bool,
  paths: List[Str],
}

type NumericSortKey = {number: Int, raw: Str}
type TextSortKey = {key: Str, raw: Str}

pure numeric_key(line: Str) -> Int {
  let text = line.trim().words().get(0) ?? "0"
  if text.starts_with("+") { 0 } else { text.parse_int() ?? 0 }
}

pure numeric_sort_key(line: Str, stable: Bool) -> NumericSortKey {
  {number: numeric_key(line), raw: if stable { "" } else { line }}
}

pure numeric_field_sort_key(line: Str, delimiter: Str, field: Int, opts: SortOptions) -> NumericSortKey {
  {number: numeric_field_key(line, delimiter, field, opts), raw: if opts.stable { "" } else { line }}
}

pure key_index(spec: Str) -> Int {
  (((spec.split(",").get(0) ?? "1").split(".").get(0) ?? "1").parse_int() ?? 1) - 1
}

pure is_ascii_digit(byte: Int) -> Bool {
  byte >= 48 and byte <= 57
}

pure is_ascii_letter(byte: Int) -> Bool {
  (byte >= 65 and byte <= 90) or (byte >= 97 and byte <= 122)
}

pure version_hex_byte(byte: Int) -> Str {
  let digits = "0123456789ABCDEF"
  digits.byte_slice(byte / 16, length: 1) + digits.byte_slice(byte % 16, length: 1)
}

## Encode natural version chunks into a lexically sortable key without narrowing numeric runs.
pure version_key(line: Str) -> Str {
  if line == "" { return "00" }
  if line == "." { return "01" }
  if line == ".." { return "02" }

  var remainder = line
  var key = ""
  while remainder.starts_with(".") {
    key += "03"
    remainder = remainder.byte_slice(1)
  }
  key += "04"

  let input = bytes.from_text(remainder)
  var at = 0
  while at < input.len() {
    let byte = input.byte_at(at) ?? 0
    if is_ascii_digit(byte) {
      var end = at + 1
      while end < input.len() and is_ascii_digit(input.byte_at(end) ?? 0) { end += 1 }

      var significant = at
      while significant < end and input.byte_at(significant) == 48 { significant += 1 }
      let significant_length = end - significant
      key += "01"
      for _ in range(significant_length) { key += "1" }
      key += "0"
      while significant < end {
        key += remainder.byte_slice(significant, length: 1)
        significant += 1
      }
      at = end
    } else if byte == 126 {
      key += "00"
      at += 1
    } else {
      key += if is_ascii_letter(byte) { "02" } else { "03" }
      key += version_hex_byte(byte)
      at += 1
    }
  }
  key + "01"
}

pure version_sort_key(line: Str, stable: Bool) -> TextSortKey {
  {key: version_key(line), raw: if stable { "" } else { line }}
}

pure version_field_sort_key(line: Str, delimiter: Str, field: Int, opts: SortOptions) -> TextSortKey {
  let parts = if delimiter == "" { line.trim().words() } else { line.split(delimiter) }
  {key: version_key(parts.get(field) ?? ""), raw: if opts.stable { "" } else { line }}
}

pure selected_sort_mode(opts: SortOptions) -> Str {
  opts.sort_mode.get(opts.sort_mode.len() - 1) ?? ""
}

pure is_version_sort(opts: SortOptions) -> Bool {
  opts.version_sort or selected_sort_mode(opts).starts_with("v")
}

pure character_order_key(text: Str, dictionary: Bool, ignore_nonprinting: Bool, fold_case: Bool) -> Str {
  if ! dictionary and ! ignore_nonprinting {
    return if fold_case { text.upper() } else { text }
  }

  let input = bytes.from_text(text)
  var key = ""
  for index in range(input.len()) {
    let byte = input.byte_at(index) ?? 0
    let blank = byte == 9 or byte == 32
    let alphanumeric = (byte >= 48 and byte <= 57) or (byte >= 65 and byte <= 90) or (byte >= 97 and byte <= 122)
    let in_dictionary = ! dictionary or blank or alphanumeric
    let printable = byte >= 32 and byte <= 126
    let in_printable = ! ignore_nonprinting or printable
    if in_dictionary and in_printable {
      key += text.byte_slice(index, length: 1)
    }
  }
  if fold_case { key.upper() } else { key }
}

pure field_key(line: Str, delimiter: Str, field: Int, opts: SortOptions) -> Str {
  let parts = if delimiter == "" { line.trim().words() } else { line.split(delimiter) }
  character_order_key(parts.get(field) ?? "", opts.dictionary, opts.ignore_nonprinting, opts.fold_case)
}

pure field_sort_key(line: Str, delimiter: Str, field: Int, opts: SortOptions) -> TextSortKey {
  {key: field_key(line, delimiter, field, opts), raw: if opts.stable { "" } else { line }}
}

pure numeric_field_key(line: Str, delimiter: Str, field: Int, opts: SortOptions) -> Int {
  field_key(line, delimiter, field, opts).parse_int() ?? 0
}

pure trim_leading_blanks(line: Str) -> Str {
  var at = 0
  while at < line.byte_len() and line.byte_slice(at, length: 1) in [" ", "\t"] { at += 1 }
  line.byte_slice(at)
}

pure blank_text_key(line: Str, opts: SortOptions) -> Str {
  let key = trim_leading_blanks(line)
  character_order_key(key, opts.dictionary, opts.ignore_nonprinting, opts.fold_case)
}

pure blank_sort_key(line: Str, opts: SortOptions) -> TextSortKey {
  {key: blank_text_key(line, opts), raw: if opts.stable { "" } else { line }}
}

## GNU sort uses the full line as a last-resort key after the blank-skipping key.
pure blank_sorted(lines: List[Str], reverse: Bool, opts: SortOptions) -> List[Str] {
  if opts.stable {
    if reverse {
      lines |> sort-by(desc: true) blank_text_key(., opts)
    } else {
      lines |> sort-by blank_text_key(., opts)
    }
  } else {
  let fallback = if reverse { lines |> sort-by(desc: true) . } else { lines |> sort }
  if reverse {
    fallback |> sort-by(desc: true) blank_text_key(., opts)
  } else {
    fallback |> sort-by blank_text_key(., opts)
  }
  }
}

pure pair_is_ordered(left: Str, right: Str, opts: SortOptions, has_key: Bool, key_field: Int) -> Bool {
  let pair = [left, right]
  let ordered = if is_version_sort(opts) and has_key {
    if opts.reverse { pair |> sort-by(desc: true) version_field_sort_key(., opts.delimiter, key_field, opts) } else { pair |> sort-by version_field_sort_key(., opts.delimiter, key_field, opts) }
  } else if is_version_sort(opts) {
    if opts.reverse { pair |> sort-by(desc: true) version_sort_key(., opts.stable) } else { pair |> sort-by version_sort_key(., opts.stable) }
  } else if opts.numeric and has_key {
    if opts.reverse { pair |> sort-by(desc: true) numeric_field_sort_key(., opts.delimiter, key_field, opts) } else { pair |> sort-by numeric_field_sort_key(., opts.delimiter, key_field, opts) }
  } else if has_key {
    if opts.reverse { pair |> sort-by(desc: true) field_sort_key(., opts.delimiter, key_field, opts) } else { pair |> sort-by field_sort_key(., opts.delimiter, key_field, opts) }
  } else if opts.numeric {
    if opts.reverse { pair |> sort-by(desc: true) numeric_sort_key(., opts.stable) } else { pair |> sort-by numeric_sort_key(., opts.stable) }
  } else if opts.blank {
    blank_sorted(pair, opts.reverse, opts)
  } else if opts.fold_case or opts.dictionary or opts.ignore_nonprinting {
    if opts.reverse {
      pair |> sort-by(desc: true) { |line| {key: character_order_key(line, opts.dictionary, opts.ignore_nonprinting, opts.fold_case), raw: if opts.stable { "" } else { line }} }
    } else {
      pair |> sort-by { |line| {key: character_order_key(line, opts.dictionary, opts.ignore_nonprinting, opts.fold_case), raw: if opts.stable { "" } else { line }} }
    }
  } else if opts.reverse {
    pair |> sort-by(desc: true) .
  } else {
    pair |> sort
  }
  ordered[0] == left
}

pure same_sort_key(left: Str, right: Str, opts: SortOptions, has_key: Bool, key_field: Int) -> Bool {
  if is_version_sort(opts) and has_key {
    version_key((if opts.delimiter == "" { left.trim().words() } else { left.split(opts.delimiter) }).get(key_field) ?? "") == version_key((if opts.delimiter == "" { right.trim().words() } else { right.split(opts.delimiter) }).get(key_field) ?? "")
  } else if is_version_sort(opts) {
    version_key(left) == version_key(right)
  } else if opts.numeric and has_key {
    numeric_field_key(left, opts.delimiter, key_field, opts) == numeric_field_key(right, opts.delimiter, key_field, opts)
  } else if has_key {
    field_key(left, opts.delimiter, key_field, opts) == field_key(right, opts.delimiter, key_field, opts)
  } else if opts.numeric {
    numeric_key(left) == numeric_key(right)
  } else if opts.blank {
    blank_text_key(left, opts) == blank_text_key(right, opts)
  } else if opts.fold_case or opts.dictionary or opts.ignore_nonprinting {
    character_order_key(left, opts.dictionary, opts.ignore_nonprinting, opts.fold_case) == character_order_key(right, opts.dictionary, opts.ignore_nonprinting, opts.fold_case)
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

## A required output operand may start with a dash, so attach it before parsing options.
pure normalize_output_args(argv: List[Str]) -> List[Str] {
  var normalized: List[Str] = []
  var operands_only = false
  var at = 0
  while at < argv.len() {
    let arg = argv[at]
    if ! operands_only and arg == "--" {
      normalized += [arg]
      operands_only = true
      at += 1
    } else if ! operands_only and arg in ["-o", "--output"] and at + 1 < argv.len() and argv[at + 1].starts_with("--") {
      let value = argv[at + 1]
      normalized += [if arg == "-o" { f"-o{value}" } else { f"--output={value}" }]
      at += 2
    } else {
      normalized += [arg]
      at += 1
    }
  }
  normalized
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
    normalize_output_args(argv),
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
      version_sort: {
        form: "-V --version-sort",
        default: false,
      },
      sort_mode: {
        form: "--sort=MODE",
        repeated: true,
      },
      fold_case: {
        form: "-f --ignore-case",
        default: false,
      },
      dictionary: {
        form: "-d --dictionary-order",
        default: false,
      },
      ignore_nonprinting: {
        form: "-i --ignore-nonprinting",
        default: false,
      },
      blank: {
        form: "-b --ignore-leading-blanks",
        default: false,
      },
      stable: {
        form: "-s --stable",
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
        repeated: true,
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
      version: {
        form: "--version",
        default: false,
        stop: true,
      },
      paths: {
        form: "...FILE",
      },
    },
  )?

  if opts.version {
    gnu.version("sort")
    return
  }

  let selected_mode = selected_sort_mode(opts)
  if selected_mode != "" and ! selected_mode.starts_with("v") {
    gnu.error(f"invalid argument {gnu.quote_maybe(selected_mode)} for '--sort'")
    exit 2
  }

  if opts.numeric and (opts.dictionary or opts.ignore_nonprinting) {
    let conflict = if opts.dictionary { "-dn" } else { "-in" }
    gnu.error(f"options '{conflict}' are incompatible")
    exit 2
  }

  let has_key = opts.key != ""
  let key_field = if has_key { key_index(opts.key) } else { 0 }
  let output_path = opts.output.get(0) ?? ""
  let has_output = ! opts.output.is_empty()
  for candidate in opts.output[1..] {
    if candidate != output_path {
      gnu.error("multiple output files specified")
      exit 2
    }
  }
  let output = if has_output { fp"{output_path}" } else { p"" }
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

  let sorted = if is_version_sort(opts) and has_key {
    if opts.reverse {
      input_lines |> sort-by(desc: true) version_field_sort_key(., delimiter, key_field, opts)
    } else {
      input_lines |> sort-by version_field_sort_key(., delimiter, key_field, opts)
    }
  } else if is_version_sort(opts) {
    if opts.reverse {
      input_lines |> sort-by(desc: true) version_sort_key(., opts.stable)
    } else {
      input_lines |> sort-by version_sort_key(., opts.stable)
    }
  } else if opts.numeric and has_key {
    if opts.reverse {
      input_lines |> sort-by(desc: true) numeric_field_sort_key(., delimiter, key_field, opts)
    } else {
      input_lines |> sort-by numeric_field_sort_key(., delimiter, key_field, opts)
    }
  } else if has_key {
    if opts.reverse {
      input_lines |> sort-by(desc: true) field_sort_key(., delimiter, key_field, opts)
    } else {
      input_lines |> sort-by field_sort_key(., delimiter, key_field, opts)
    }
  } else if opts.numeric {
    if opts.reverse {
      input_lines |> sort-by(desc: true) numeric_sort_key(., opts.stable)
    } else {
      input_lines |> sort-by numeric_sort_key(., opts.stable)
    }
  } else if opts.blank {
    blank_sorted(input_lines, opts.reverse, opts)
  } else if opts.fold_case or opts.dictionary or opts.ignore_nonprinting {
    if opts.reverse {
      input_lines |> sort-by(desc: true) { |line| {key: character_order_key(line, opts.dictionary, opts.ignore_nonprinting, opts.fold_case), raw: if opts.stable { "" } else { line }} }
    } else {
      input_lines |> sort-by { |line| {key: character_order_key(line, opts.dictionary, opts.ignore_nonprinting, opts.fold_case), raw: if opts.stable { "" } else { line }} }
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
    if let Err(failure) = output.write(text) {
      let action = if let Ok(_) = fs.stat(output, follow_symlinks: true) { "write failed" } else { "open failed" }
      gnu.error(f"{action}: {gnu.quote_maybe(output_path)}: {gnu.strerror(failure)}")
      exit 2
    }
  } else if opts.zero_terminated {
    gnu.write_text(text)
  } else {
    for line in lines {
      print $line
    }
  }
}
