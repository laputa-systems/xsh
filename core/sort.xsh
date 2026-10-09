#!/bin/xsh
use lib.gnu

const USAGE = """Usage: sort [OPTION]... [FILE]...
Write sorted concatenation of all FILE(s) to standard output.

With no FILE, or when FILE is -, read standard input.

Ordering options:
  -b, --ignore-leading-blanks  ignore leading blanks
  -d, --dictionary-order       consider only blanks and alphanumeric characters
  -f, --ignore-case            fold lower case to upper case characters
  -i, --ignore-nonprinting     consider only printable characters
  -n, --numeric-sort           compare according to string numerical value
  -M, --month-sort             compare (unknown) < 'JAN' < ... < 'DEC'
  -V, --version-sort           natural sort of (version) numbers within text
  -r, --reverse                reverse the result of comparisons
  -s, --stable                 stabilize sort by disabling last-resort comparison
  -u, --unique                 output only the first of an equal run

Other options:
  -c, --check[=DIAGNOSE_FIRST] check for sorted input; do not sort
  -C, --check=silent           like -c, but do not report the first bad line
  -k, --key=KEYDEF             sort via a key; KEYDEF gives start and stop positions
  -m, --merge                  merge already sorted files; do not sort
  -o, --output=FILE            write result to FILE instead of standard output
  -t, --field-separator=SEP    use SEP instead of non-blank to blank transition
  -S, --buffer-size=SIZE       set maximum size for in-memory sort runs
  -T, --temporary-directory=DIR use DIR for temporary files
  -z, --zero-terminated        line delimiter is NUL, not newline
      --parallel=NUM_THREADS   set number of parallel threads
      --help                   display this help and exit
      --version                output version information and exit
"""

type SortOptions = {
  reverse: Bool,
  unique: Bool,
  numeric: Bool,
  month: Bool,
  version_sort: Bool,
  fold_case: Bool,
  blank: Bool,
  dictionary: Bool,
  nonprinting: Bool,
  stable: Bool,
  merge: Bool,
  debug: Bool,
  check: Bool,
  check_mode: Str?,
  check_silent: Bool,
  key: List[Str],
  delimiter: Str?,
  output: List[Str],
  zero: Bool,
  parallel: Int?,
  sort_mode: Str?,
  buffer_size: Str?,
  batch_size: Str?,
  temp_dir: Str?,
  files0_from: Str?,
  help: Bool,
  version: Bool,
  paths: List[Str],
}

type KeyPart = {field: Int, character: Int, flags: Str}
type KeyDef = {start: KeyPart, finish: KeyPart, has_finish: Bool}
type SortLine = {raw: Bytes, primary: Bytes, ordering: Bytes}
type Bounds = {start: Int, end: Int}
type NumericMagnitude = {negative: Bool, magnitude: Str}
type SelectedKey = {value: Bytes, flags: Str}
type SizeResult = {bytes: Int?, error: Str}

pure is_digit(byte: Int) -> Bool { byte >= 48 and byte <= 57 }

pure is_blank(byte: Int) -> Bool { byte == 32 or byte == 9 }

pure chr(code: Int) -> Str { "0123456789".byte_slice(code, length: 1) }

pure byte_order(left: Bytes, right: Bytes) -> Int {
  let found = left.compare(right)
  return 0 when found.equal
  if found.left < found.right { -1 } else { 1 }
}

pure hex_digit(value: Int) -> Str {
  "0123456789abcdef".byte_slice(value, length: 1)
}

pure parse_buffer_size(value: Str) -> SizeResult {
  var at = 0
  var digits = ""
  while at < value.byte_len() and is_digit(value.byte_at(at) ?? 0) {
    digits = f"{digits}{value.byte_slice(at, length: 1)}"
    at += 1
  }
  if digits == "" { return {bytes: null, error: "invalid"} }
  let amount = digits.parse_int() ?? -1
  if amount < 0 { return {bytes: null, error: "large"} }
  let suffix = value.byte_slice(at)
  if suffix.ends_with("%") {
    if suffix == "%" { return {bytes: if amount == 0 { 0 } else { 9223372036854775807 }, error: ""} }
    return {bytes: null, error: "invalid"}
  }
  if suffix == "" { return {bytes: amount, error: ""} }
  if suffix == "b" {
    if amount > 9223372036854775807 / 512 { return {bytes: null, error: "large"} }
    return {bytes: amount * 512, error: ""}
  }

  var unit = suffix
  var decimal = false
  if suffix.ends_with("iB") {
    unit = suffix.byte_slice(0, suffix.byte_len() - 2)
  } else if suffix.ends_with("B") {
    unit = suffix.byte_slice(0, suffix.byte_len() - 1)
    decimal = true
  }
  if unit.byte_len() != 1 { return {bytes: null, error: "suffix"} }
  let upper = unit.upper()
  let units = ["K", "M", "G", "T", "P", "E", "Z", "Y", "R", "Q"]
  var power = -1
  for index in range(units.len()) { if units[index] == upper { power = index + 1; break } }
  if power < 0 { return {bytes: null, error: "suffix"} }

  let base = if decimal or unit != upper { 1000 } else { 1024 }
  var factor = 1
  for _ in range(power) {
    if factor > 9223372036854775807 / base { return {bytes: null, error: "large"} }
    factor *= base
  }
  if amount > 9223372036854775807 / factor { return {bytes: null, error: "large"} }
  {bytes: amount * factor, error: ""}
}

pure parse_batch_size(value: Str) -> SizeResult {
  return {bytes: null, error: "invalid"} when value == ""
  for index in range(value.byte_len()) {
    if ! is_digit(value.byte_at(index) ?? 0) { return {bytes: null, error: "invalid"} }
  }
  let count = value.parse_int() ?? -1
  return {bytes: null, error: "large"} when count < 0
  return {bytes: null, error: "small"} when count < 2
  {bytes: count, error: ""}
}

pure hex_bytes(value: Bytes) -> Str {
  var out = ""
  for index in range(value.len()) {
    let byte = value.byte_at(index) ?? 0
    out = f"{out}{hex_digit(byte / 16)}{hex_digit(byte % 16)}"
  }
  out
}

pure inverted_hex(value: Str) -> Str {
  var out = ""
  for index in range(value.byte_len()) {
    let ch = value.byte_at(index) ?? 0
    if ch >= 48 and ch <= 57 {
      out = f"{out}{hex_digit(15 - (ch - 48))}"
    } else if ch >= 97 and ch <= 102 {
      out = f"{out}{hex_digit(15 - (ch - 87))}"
    } else {
      out = f"{out}~"
    }
  }
  out
}

pure inverted_numeric(value: Str) -> Str {
  var out = ""
  for index in range(value.byte_len()) {
    let ch = value.byte_at(index) ?? 0
    if ch >= 48 and ch <= 57 {
      out = f"{out}{chr(9 - (ch - 48))}"
    } else if ch == 97 {
      out = f"{out}y"
    } else {
      out = f"{out}~"
    }
  }
  out
}

pure numeric_magnitude(value: Bytes) -> NumericMagnitude {
  var at = 0
  while at < value.len() and is_blank(value.byte_at(at) ?? 0) { at += 1 }
  let negative = at < value.len() and (value.byte_at(at) ?? 0) == 45
  if negative { at += 1 }

  var integer = ""
  while at < value.len() and is_digit(value.byte_at(at) ?? 0) {
    integer = f"{integer}{value[at..at + 1].utf8() ?? "0"}"
    at += 1
  }

  var fraction = ""
  if at < value.len() and (value.byte_at(at) ?? 0) == 46 {
    at += 1
    while at < value.len() and is_digit(value.byte_at(at) ?? 0) {
      fraction = f"{fraction}{value[at..at + 1].utf8() ?? "0"}"
      at += 1
    }
  }

  var first = 0
  while first < integer.byte_len() and integer.byte_at(first) == 48 { first += 1 }
  integer = if first == integer.byte_len() { "0" } else { integer.byte_slice(first) }
  while fraction.ends_with("0") { fraction = fraction.byte_slice(0, fraction.byte_len() - 1) }

  var magnitude = ""
  for _ in range(integer.byte_len()) { magnitude = f"{magnitude}a" }
  magnitude = f"{magnitude}!{integer}!{fraction}!"

  {negative: negative and (integer != "0" or fraction != ""), magnitude: magnitude}
}

pure numeric_key(value: Bytes) -> Str {
  let parts = numeric_magnitude(value)
  if parts.negative {
    f"0{inverted_numeric(parts.magnitude)}"
  } else {
    f"1{parts.magnitude}"
  }
}

pure month_key(value: Bytes) -> Str {
  var at = 0
  while at < value.len() and is_blank(value.byte_at(at) ?? 0) { at += 1 }
  let token = value[at..if at + 3 > value.len() { value.len() } else { at + 3 }].utf8() ?? ""
  let month = token.upper()
  let months = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"]
  let codes = ["01", "02", "03", "04", "05", "06", "07", "08", "09", "10", "11", "12"]
  var index = 0
  while index < months.len() {
    if month == months[index] { return codes[index] }
    index += 1
  }
  "0"
}

pure version_key(value: Bytes) -> Str {
  var out = ""
  var at = 0
  while at < value.len() {
    let digits = is_digit(value.byte_at(at) ?? 0)
    var end = at + 1
    while end < value.len() and is_digit(value.byte_at(end) ?? 0) == digits { end += 1 }
    let part = value[at..end]
    if digits {
      var first = 0
      while first + 1 < part.len() and part.byte_at(first) == 48 { first += 1 }
      let number = part[first..].utf8() ?? "0"
      var count = 0
      while first > 0 { count += 1; first -= 1 }
      var length = ""
      for _ in range(number.byte_len()) { length = f"{length}a" }
      var leading_zeroes = ""
      for _ in range(count) { leading_zeroes = f"{leading_zeroes}a" }
      out = f"{out}0{length}!{number}!{leading_zeroes}z!"
    } else {
      out = f"{out}1{hex_bytes(part)}!"
    }
    at = end
  }
  out
}

pure parse_key_part(value: Str) -> KeyPart {
  var at = 0
  var field_text = ""
  while at < value.byte_len() and is_digit(value.byte_at(at) ?? 0) {
    field_text = f"{field_text}{value.byte_slice(at, length: 1)}"
    at += 1
  }
  let field = (field_text.parse_int() ?? 1) - 1
  var character = 0
  var flags = ""
  if at < value.byte_len() and value.byte_slice(at, length: 1) == "." {
    at += 1
    var char_text = ""
    while at < value.byte_len() and is_digit(value.byte_at(at) ?? 0) {
      char_text = f"{char_text}{value.byte_slice(at, length: 1)}"
      at += 1
    }
    character = char_text.parse_int() ?? 0
  }
  flags = value.byte_slice(at)
  {field: field, character: character, flags: flags}
}

pure parse_key(value: Str) -> KeyDef {
  let halves = value.split(",")
  let start = parse_key_part(halves[0])
  let has_finish = halves.len() > 1
  let finish = if has_finish { parse_key_part(halves[1]) } else { {field: 0, character: 0, flags: ""} }
  {start: start, finish: finish, has_finish: has_finish}
}

pure has_zero_character(value: Str) -> Bool { value.find(".0") != null }

pure field_bounds(line: Bytes, separator: Int, wanted: Int) -> Bounds {
  if separator >= 0 {
    var field = 0
    var start = 0
    for index in range(line.len()) {
      if line.byte_at(index) == separator {
        if field == wanted { return {start: start, end: index} }
        field += 1
        start = index + 1
      }
    }
    return if field == wanted { {start: start, end: line.len()} } else { {start: line.len(), end: line.len()} }
  }

  var field = 0
  var at = 0
  while at < line.len() {
    while at < line.len() and is_blank(line.byte_at(at) ?? 0) { at += 1 }
    if at >= line.len() { break }
    let start = at
    while at < line.len() and ! is_blank(line.byte_at(at) ?? 0) { at += 1 }
    if field == wanted { return {start: start, end: at} }
    field += 1
  }
  {start: line.len(), end: line.len()}
}

pure selected_key(line: Bytes, spec: Str, separator: Int, leading_blanks: Bool) -> SelectedKey {
  let definition = parse_key(spec)
  let start_bounds = field_bounds(line, separator, definition.start.field)
  var start = start_bounds.start + (if definition.start.character > 0 { definition.start.character - 1 } else { 0 })
  var end = line.len()

  if definition.has_finish {
    let finish_bounds = field_bounds(line, separator, definition.finish.field)
    end = finish_bounds.end
    if definition.finish.character > 0 {
      let requested = finish_bounds.start + definition.finish.character
      end = if requested < end { requested } else { end }
    }
  }

  if definition.start.field < 0 or (definition.start.character == 0 and spec.find(".0") != null) {
    return {value: b"", flags: definition.start.flags}
  }
  if start > end { start = end }
  if leading_blanks or definition.start.flags.find("b") != null {
    while start < end and is_blank(line.byte_at(start) ?? 0) { start += 1 }
  }
  {value: line[start..end], flags: f"{definition.start.flags}{definition.finish.flags}"}
}

pure text_key(value: Bytes, fold_case: Bool, dictionary: Bool, nonprinting: Bool, blank: Bool) -> Str {
  var out = ""
  var leading = blank
  for index in range(value.len()) {
    var byte = value.byte_at(index) ?? 0
    if leading and is_blank(byte) { continue }
    leading = false
    if fold_case and byte >= 65 and byte <= 90 { byte += 32 }
    let printable = byte >= 32 and byte <= 126
    let accepted = ! dictionary or is_blank(byte) or (byte >= 48 and byte <= 57) or (byte >= 65 and byte <= 90) or (byte >= 97 and byte <= 122)
    continue when (nonprinting and ! printable) or ! accepted
    if fold_case {
      let letter = (byte >= 65 and byte <= 90) or (byte >= 97 and byte <= 122)
      out = f"{out}{if letter { "0" } else { "1" }}{hex_digit(byte / 16)}{hex_digit(byte % 16)}"
    } else {
      out = f"{out}{hex_digit(byte / 16)}{hex_digit(byte % 16)}"
    }
  }
  out
}

pure primary_key(line: Bytes, keys: List[Str], separator: Int, numeric: Bool, month: Bool, version_sort: Bool, fold_case: Bool, dictionary: Bool, nonprinting: Bool, blank: Bool) -> Str {
  if keys.len() == 0 {
    if numeric { return f"{numeric_key(line)}!" }
    if month { return f"{month_key(line)}!" }
    if version_sort { return f"{version_key(line)}!" }
    return f"{text_key(line, fold_case, dictionary, nonprinting, blank)}!"
  }

  var out = ""
  for spec in keys {
    let selected = selected_key(line, spec, separator, blank)
    let local_numeric = numeric or selected.flags.find("n") != null
    let local_month = month or selected.flags.find("M") != null
    let local_version = version_sort or selected.flags.find("V") != null
    var piece = if local_numeric { numeric_key(selected.value) } else if local_month { month_key(selected.value) } else if local_version { version_key(selected.value) } else {
      text_key(
        selected.value,
        fold_case or selected.flags.find("f") != null,
        dictionary or selected.flags.find("d") != null,
        nonprinting or selected.flags.find("i") != null,
        blank or selected.flags.find("b") != null,
      )
    }
    let local_reverse = selected.flags.find("r") != null
    if local_reverse { piece = if local_numeric { inverted_numeric(piece) } else { inverted_hex(piece) } }
    out = f"{out}{piece}{if local_reverse { "~" } else { "!" }}"
  }
  out
}

pure split_records(data: Bytes, separator: Int) -> List[Bytes] {
  var records: List[Bytes] = []
  var start = 0
  for index in range(data.len()) {
    if data.byte_at(index) == separator {
      records += [data[start..index]]
      start = index + 1
    }
  }
  if start < data.len() { records += [data[start..]] }
  records
}

pure merge_groups(groups: List[List[SortLine]], reverse: Bool) -> List[SortLine] {
  var positions: List[Int] = [0 for _ in range(groups.len())]
  var total = 0
  for file_set in groups { total += file_set.len() }
  var merged: List[SortLine] = []
  while merged.len() < total {
    var selected_group = -1
    for group_index in range(groups.len()) {
      let position = positions[group_index]
      continue when position >= groups[group_index].len()
      if selected_group < 0 {
        selected_group = group_index
      } else {
        let candidate = groups[group_index][position]
        let current = groups[selected_group][positions[selected_group]]
        let order = byte_order(candidate.ordering, current.ordering)
        if (reverse and order > 0) or (! reverse and order < 0) {
          selected_group = group_index
        }
      }
    }
    merged += [groups[selected_group][positions[selected_group]]]
    positions[selected_group] += 1
  }
  merged
}

pure merge_batches(groups: List[List[SortLine]], reverse: Bool, batch_size: Int) -> List[SortLine] {
  var active = groups
  while active.len() > 1 {
    var next: List[List[SortLine]] = []
    var start = 0
    while start < active.len() {
      let end = if start + batch_size < active.len() { start + batch_size } else { active.len() }
      var batch: List[List[SortLine]] = []
      for index in range(start, end) { batch += [active[index]] }
      next += [merge_groups(batch, reverse)]
      start = end
    }
    active = next
  }
  if active.len() == 0 { [] } else { active[0] }
}

pure separator_bytes(zero: Bool) -> Bytes { if zero { b"\0" } else { b"\n" } }

pure debug_line(line: Bytes, blank: Bool, tie: Bool) -> Bytes {
  var marks = ""
  var whole_line_marks = ""
  var leading = blank
  for index in range(line.len()) {
    let byte = line.byte_at(index) ?? 0
    whole_line_marks = f"{whole_line_marks}{if byte == 9 { ">" } else { "_" }}"
    if leading and is_blank(byte) {
      marks = f"{marks} "
    } else {
      leading = false
      marks = f"{marks}{if byte == 9 { ">" } else { "_" }}"
    }
  }
  var output = bytes.concat([line, b"\n", bytes.from_text(f"{marks}\n")])
  if tie { output = bytes.concat([output, bytes.from_text(f"{whole_line_marks}\n")]) }
  output
}

proc read_input(name: Str) [fs, error, io] -> Result[Bytes, Error] {
  gnu.read_operand(name)
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: SortOptions = cli.applet(
    argv,
    {
      gnu: {
        status: 2,
        unsupported: {
          "-g": "general numeric ordering is not implemented",
          "--general-numeric-sort": "general numeric ordering is not implemented",
          "-h": "human numeric ordering is not implemented",
          "--human-numeric-sort": "human numeric ordering is not implemented",
          "-R": "random ordering is not implemented",
          "--random-source": "random ordering is not implemented",
          "--compress-program": "external sorting is not implemented",
        },
      },
      reverse: {form: "-r --reverse", default: false},
      unique: {form: "-u --unique", default: false},
      numeric: {form: "-n --numeric-sort", default: false},
      month: {form: "-M --month-sort", default: false},
      version_sort: {form: "-V --version-sort", default: false},
      fold_case: {form: "-f --ignore-case", default: false},
      blank: {form: "-b --ignore-leading-blanks", default: false},
      dictionary: {form: "-d --dictionary-order", default: false},
      nonprinting: {form: "-i --ignore-nonprinting", default: false},
      stable: {form: "-s --stable", default: false},
      merge: {form: "-m --merge", default: false},
      debug: {form: "--debug", default: false},
      check: {form: "-c", default: false},
      check_mode: {form: "--check[=MODE]", optional_value: true, optional_default: "diagnose-first"},
      check_silent: {form: "-C", default: false},
      key: {form: "-k --key KEYDEF", repeated: true},
      delimiter: {form: "-t --field-separator SEP"},
      output: {form: "-o --output FILE", repeated: true},
      zero: {form: "-z --zero-terminated", default: false},
      parallel: {form: "--parallel NUM_THREADS", kind: "Int"},
      sort_mode: {form: "--sort MODE"},
      buffer_size: {form: "-S --buffer-size SIZE"},
      batch_size: {form: "--batch-size SIZE"},
      temp_dir: {form: "-T --temporary-directory DIR"},
      files0_from: {form: "--files0-from FILE"},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      paths: {form: "...FILE"},
    },
  )?

  if opts.help { gnu.help(USAGE); return }
  if opts.version { gnu.version("sort"); return }

  if opts.parallel != null {
    let threads = opts.parallel ?? 1
    if threads < 1 { gnu.usage_error("invalid --parallel argument", 2) }
    # The option controls worker count in GNU sort. This implementation sorts
    # in one process-wide list, so a positive count has no effect on ordering.
  }

  var buffer_limit: Int? = null
  if opts.buffer_size != null {
    let size_text = opts.buffer_size ?? ""
    let parsed = parse_buffer_size(size_text)
    if parsed.error == "invalid" {
      gnu.error(f"invalid --buffer-size argument {gnu.quote_value(size_text)}")
      exit 2
    }
    if parsed.error == "suffix" {
      gnu.error(f"invalid suffix in --buffer-size argument {gnu.quote_value(size_text)}")
      exit 2
    }
    if parsed.error == "large" {
      gnu.error(f"--buffer-size argument {gnu.quote_value(size_text)} too large")
      exit 2
    }
    buffer_limit = parsed.bytes
  }

  var batch_limit: Int? = null
  if opts.batch_size != null {
    let size_text = opts.batch_size ?? ""
    let parsed = parse_batch_size(size_text)
    if parsed.error == "invalid" {
      gnu.error(f"invalid --batch-size argument {gnu.quote_value(size_text)}")
      exit 2
    }
    if parsed.error == "small" {
      gnu.error(f"invalid --batch-size argument {gnu.quote_value(size_text)}")
      eprint "sort: minimum --batch-size argument is '2'"
      exit 2
    }
    let nofile = process.rlimit("nofile")?
    let max_batch = if nofile.soft == null { 9223372036854775807 } else {
      let soft = nofile.soft ?? 0
      if soft > 3 { soft - 3 } else { 0 }
    }
    let count = parsed.bytes ?? 0
    if parsed.error == "large" or count > max_batch {
      gnu.error(f"--batch-size argument {gnu.quote_value(size_text)} too large")
      eprint f"sort: maximum --batch-size argument with current rlimit is {max_batch}"
      exit 2
    }
    batch_limit = parsed.bytes
  }

  var numeric = opts.numeric
  var month = opts.month
  var version_sort = opts.version_sort
  if opts.sort_mode != null {
    let mode = opts.sort_mode ?? ""
    if "numeric".starts_with(mode) { numeric = true } else if "month".starts_with(mode) { month = true } else if "version".starts_with(mode) { version_sort = true } else if "random".starts_with(mode) { gnu.usage_error("option '--sort=random' is not supported: random ordering is not implemented", 2) } else if "general-numeric".starts_with(mode) { gnu.usage_error("option '--sort=general-numeric' is not supported: general numeric ordering is not implemented", 2) } else if "human-numeric".starts_with(mode) { gnu.usage_error("option '--sort=human-numeric' is not supported: human numeric ordering is not implemented", 2) } else { gnu.usage_error(f"invalid argument {gnu.quote_value(mode)} for '--sort'", 2) }
  }

  if (if numeric { 1 } else { 0 }) + (if month { 1 } else { 0 }) + (if version_sort { 1 } else { 0 }) > 1 {
    gnu.usage_error("options '-n', '-M', and '-V' are incompatible", 2)
  }

  let check_active = opts.check or opts.check_mode != null or opts.check_silent
  let check_mode = opts.check_mode ?? (if opts.check { "diagnose-first" } else { "" })
  let check_silent_mode = opts.check_silent or (check_mode != "" and ("silent".starts_with(check_mode) or "quiet".starts_with(check_mode)))
  let check_diagnose_mode = check_mode == "" or "diagnose-first".starts_with(check_mode)
  if (opts.check or opts.check_mode != null) and opts.check_silent { gnu.usage_error("options '-c' and '-C' are incompatible", 2) }
  if check_active and opts.output.len() > 0 {
    gnu.error(f"options '{if check_silent_mode { "-Co" } else { "-co" }}' are incompatible")
    exit 2
  }
  if check_active and opts.paths.len() > 1 { gnu.extra_operand(opts.paths[1], 2) }
  if opts.check_mode != null and ! check_silent_mode and ! check_diagnose_mode {
    gnu.usage_error(f"invalid argument {gnu.quote_value(check_mode)} for '--check'", 2)
  }

  for spec in opts.key {
    let definition = parse_key(spec)
    let parts = spec.split(",")
    let start_invalid = definition.start.field < 0 or (has_zero_character(parts[0]) and definition.start.character == 0)
    let finish_invalid = definition.has_finish and (definition.finish.field < 0 or (definition.finish.character == 0 and has_zero_character(parts[1])))
    if start_invalid or finish_invalid {
      let bad_field = if definition.start.field < 0 { spec } else { parts[0] }
      gnu.error(f"invalid field specification {gnu.quote_value(bad_field)}")
      exit 2
    }
    let flags = definition.start.flags + definition.finish.flags
    for index in range(flags.byte_len()) {
      if "bdfiMnrV".find(flags.byte_slice(index, length: 1)) == null { gnu.usage_error(f"invalid key specification {gnu.quote_value(spec)}", 2) }
    }
  }

  let separator = if opts.delimiter == null { -1 } else {
    let value = opts.delimiter ?? ""
    if value == "\\0" { 0 } else if value.byte_len() == 1 { value.byte_at(0) ?? 0 } else {
      gnu.usage_error(f"separator must be exactly one character long: {gnu.quote_value(value)}", 2)
      0
    }
  }

  var paths = if opts.files0_from == null { opts.paths } else {
    if opts.paths.len() > 0 {
      gnu.error(f"extra operand {gnu.quote(opts.paths[0])}")
      eprint "file operands cannot be combined with --files0-from"
      exit 2
    }
    let source_name = opts.files0_from ?? ""
    if source_name != "-" {
      if let Ok(info) = fs.stat(fp"{source_name}") {
        if info.kind == "dir" {
          gnu.error(f"cannot read: {gnu.quote_maybe(source_name)}: Is a directory")
          exit 2
        }
      }
    }
    guard let data = read_input(source_name) else { |failure|
      if fp"{source_name}".exists()? {
        gnu.error(f"cannot read: {gnu.quote_maybe(source_name)}: {gnu.strerror(failure)}")
      } else {
        gnu.error(f"open failed: {gnu.quote_maybe(source_name)}: {gnu.strerror(failure)}")
      }
      exit 2
    }
    if data.len() == 0 {
      gnu.error(f"no input from {gnu.quote(source_name)}")
      exit 2
    }
    var names: List[Str] = []
    var line_number = 0
    for record in split_records(data, 0) {
      line_number += 1
      if record.len() == 0 {
        gnu.error(f"{gnu.quote_maybe(source_name)}:{line_number}: invalid zero-length file name")
        exit 2
      }
      guard let name = record.utf8() else {
        gnu.error(f"cannot read: {gnu.quote_maybe(source_name)}: invalid UTF-8 file name")
        exit 2
      }
      if name == "-" {
        if source_name == "-" {
          gnu.error("when reading file names from standard input, no file name of '-' allowed")
        } else {
          gnu.error("-: cannot read file list from standard input and also use it as an input file")
        }
        exit 2
      }
      names += [name]
    }
    names
  }
  if paths.len() == 0 { paths = ["-"] }

  # Check every named input before reading any of them. Besides matching GNU's
  # diagnostics, this prevents an earlier /dev/random operand from hiding a
  # later missing-file error.
  for name in paths {
    continue when name == "-"
    if ! fp"{name}".exists()? {
      gnu.error(f"cannot read: {gnu.quote_maybe(name)}: No such file or directory")
      exit 2
    }
  }

  let line_separator = if opts.zero { 0 } else { 10 }
  let bytewise_default = opts.key.len() == 0 and ! numeric and ! month and ! version_sort and ! opts.fold_case and ! opts.dictionary and ! opts.nonprinting and ! opts.blank
  var record_groups: List[List[SortLine]] = []
  var input_size = 0
  for name in paths {
    guard let data = read_input(name) else { |failure|
      if gnu.errno(failure) == 5 {
        gnu.error(f"read failed: {gnu.strerror(failure)}")
      } else {
        gnu.error(f"cannot read: {gnu.quote_maybe(name)}: {gnu.strerror(failure)}")
      }
      exit 2
    }
    input_size += data.len()
    if ! check_active and buffer_limit != null and (buffer_limit ?? 0) > 0 and input_size > (buffer_limit ?? 0) {
      let temporary = opts.temp_dir ?? ""
      if temporary != "" and ! fp"{temporary}".exists()? {
        gnu.error(f"cannot create temporary file in {gnu.quote(temporary)}")
        exit 2
      }
      if temporary != "" {
        gnu.error("external sorting is not implemented for input larger than --buffer-size")
        exit 2
      }
      # With no explicit temporary directory, keep the in-memory records and
      # use the size only as the requested spill threshold.
    }
    var file_records: List[SortLine] = []
    for line in split_records(data, line_separator) {
      let primary_text = if bytewise_default { "" } else {
        primary_key(line, opts.key, separator, numeric, month, version_sort, opts.fold_case, opts.dictionary, opts.nonprinting, opts.blank)
      }
      # In default byte order, the raw line already is both the ordering and
      # unique key. Keep it as bytes instead of allocating an escaped hex copy.
      let primary = if bytewise_default { line } else { bytes.from_text(primary_text) }
      let tie = if bytewise_default or opts.stable or opts.unique { "" } else { hex_bytes(line) }
      let ordering = if bytewise_default { line } else { bytes.from_text(f"{primary_text}{tie}") }
      file_records += [{raw: line, primary: primary, ordering: ordering}]
    }
    record_groups += [file_records]
  }

  if check_active {
    let records = record_groups[0]
    var failure_index = -1
    if records.len() > 1 {
      for index in range(1, records.len()) {
        let previous = records[index - 1]
        let current = records[index]
        let order = byte_order(previous.ordering, current.ordering)
        let out_of_order = if opts.reverse { order < 0 } else { order > 0 }
        let duplicate = opts.unique and previous.primary == current.primary
        if out_of_order or duplicate { failure_index = index; break }
      }
    }
    if failure_index >= 0 {
      if ! check_silent_mode {
        let name = if paths.len() == 0 { "-" } else { paths[0] }
        let text = records[failure_index].raw.utf8() ?? ""
        gnu.error(f"{gnu.quote_maybe(name)}:{failure_index + 1}: disorder: {text}")
      }
      exit 1
    }
    return
  }

  var records: List[SortLine] = []
  if opts.merge {
    # Merge in bounded fan-in passes. This honors --batch-size and preserves
    # equal-line order across the original input streams.
    let fan_in = batch_limit ?? (if record_groups.len() > 1 { record_groups.len() } else { 2 })
    records = merge_batches(record_groups, opts.reverse, fan_in)
  } else {
    for file_set in record_groups { records += file_set }
  }
  let sorted = if opts.merge { records } else { records |> sort-by(desc: opts.reverse) .ordering }
  let selected = if opts.unique { sorted |> unique-by .primary } else { sorted }
  let ending = separator_bytes(opts.zero)
  var output: List[Bytes] = []
  for item in selected {
    if opts.debug {
      output += [debug_line(item.raw, opts.blank, ! bytewise_default and ! opts.stable and ! opts.unique)]
    } else {
      output += [item.raw, ending]
    }
  }
  let result = bytes.concat(output)

  if opts.output.len() > 1 {
    for name in opts.output[1..] {
      if name != opts.output[0] {
        gnu.error("multiple output files specified")
        exit 2
      }
    }
  }
  if opts.output.len() > 0 {
    guard let written = fp"{opts.output[0]}".write(result) else { |failure|
      let output_name = opts.output[0]
      if fp"{output_name}".exists()? {
        gnu.error(f"write failed: {gnu.quote_maybe(output_name)}: {gnu.strerror(failure)}")
      } else {
        gnu.error(f"open failed: {gnu.quote_maybe(output_name)}: {gnu.strerror(failure)}")
      }
      exit 2
    }
  } else {
    if let Err(failure) = io.write_stdout_bytes(result) {
      if gnu.errno(failure) == 32 { exit 141 }
      gnu.error(f"write failed: 'standard output': {gnu.strerror(failure)}")
      exit 2
    }
  }
}
