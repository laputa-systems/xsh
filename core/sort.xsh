#!/bin/xsh
use lib.gnu
use lib.text_a2 as text

const USAGE = """Usage: sort [OPTION]... [FILE]...
  or:  sort [OPTION]... --files0-from=F
Write sorted concatenation of all FILE(s) to standard output.

With no FILE, or when FILE is -, read standard input.

Mandatory arguments to long options are mandatory for short options too.
Ordering options:

  -b, --ignore-leading-blanks
         ignore leading blanks when finding sort keys in each line
  -d, --dictionary-order
         consider only blanks and alphanumeric characters
  -f, --ignore-case
         fold lower case to upper case characters
  -g, --general-numeric-sort
         compare according to general numerical value
  -i, --ignore-nonprinting
         consider only printable characters
  -M, --month-sort
         compare (unknown) < 'JAN' < ... < 'DEC'
  -h, --human-numeric-sort
         compare human readable numbers (e.g., 2K 1G)
  -n, --numeric-sort
         compare according to string numerical value;
         see full documentation for supported strings
  -R, --random-sort
         shuffle, but group identical keys.  See also shuf(1)
      --random-source=FILE
         get random bytes from FILE
  -r, --reverse
         reverse the result of comparisons
      --sort=WORD
         sort according to WORD:
           general-numeric -g, human-numeric -h, month -M,
           numeric -n, random -R, version -V
  -V, --version-sort
         natural sort of (version) numbers within text

Other options:

      --batch-size=NMERGE
         merge at most NMERGE inputs at once; for more use temp files
  -c, --check, --check=diagnose-first
         check for sorted input; do not sort
  -C, --check=quiet, --check=silent
         like -c, but do not report first bad line
      --compress-program=PROG
         compress temporaries with PROG; decompress them with PROG -d
      --debug
         annotate the part of the line used to sort,
         and warn about questionable usage to standard error
      --files0-from=F
         read input from the files specified by NUL-terminated names in file F;
         If F is -, read names from standard input
  -k, --key=KEYDEF
         sort via a key; KEYDEF gives location and type
  -m, --merge
         merge already sorted files; do not sort
  -o, --output=FILE
         write result to FILE instead of standard output
  -s, --stable
         stabilize sort by disabling last-resort comparison
  -S, --buffer-size=SIZE
         use SIZE for main memory buffer
  -t, --field-separator=SEP
         use SEP instead of non-blank to blank transition
  -T, --temporary-directory=DIR
         use DIR for temporaries, not $TMPDIR or /tmp;
         multiple options specify multiple directories
      --parallel=N
         change the number of sorts run concurrently to N
  -u, --unique
         output only the first of lines with equal keys;
         with -c, check for strict ordering
  -z, --zero-terminated
         line delimiter is NUL, not newline
      --help
         display this help and exit
      --version
         output version information and exit

KEYDEF is F[.C][OPTS][,F[.C][OPTS]] for start and stop position, where F is a
field number and C a character position in the field; both are origin 1, and
the stop position defaults to the line's end.  If neither -t nor -b is in
effect, characters in a field are counted from the beginning of the preceding
whitespace.  OPTS is one or more single-letter ordering options [bdfgiMhnRrV],
which override global ordering options for that key.  If no key is given, use
the entire line as the key.  Use --debug to diagnose incorrect key usage.

SIZE may be followed by the following multiplicative suffixes:
% 1% of memory, b 1, K 1024 (default), and so on for M, G, T, P, E, Z, Y, R, Q.

*** WARNING ***
The locale specified by the environment affects sort order.
Set LC_ALL=C to get the traditional sort order that uses
native byte values.

Report bugs to: bug-coreutils@gnu.org
GNU coreutils home page: <https://www.gnu.org/software/coreutils/>
General help using GNU software: <https://www.gnu.org/gethelp/>
Report any translation bugs to <https://translationproject.org/team/>
Full documentation <https://www.gnu.org/software/coreutils/sort>
or available locally via: info '(coreutils) sort invocation'
"""

type SortOptions = {
  reverse: Bool,
  unique: Bool,
  numeric: Bool,
  human_numeric: Bool,
  general_numeric: Bool,
  month: Bool,
  fold_case: Bool,
  dictionary: Bool,
  ignore_nonprinting: Bool,
  blank: Bool,
  stable: Bool,
  merge: Bool,
  version_sort: Bool,
  sort_mode: List[Str],
  buffer_size: Str?,
  batch_size: Str?,
  temp_directory: Str?,
  compress_program: Str?,
  debug: Bool,
  key: List[Str],
  delimiter: List[Str],
  parallel: List[Str],
  output: List[Str],
  check: Str,
  short_check: Bool,
  silent_check: Bool,
  zero_terminated: Bool,
  files0_from: Str?,
  help: Bool,
  version: Bool,
  random: Bool,
  random_source: Str?,
  paths: List[Str],
  general_numeric_keys: Map[GeneralNumericEntry],
  decimal_point: Str,
  thousands_separator: Bytes,
  months: List[Str],
  debug_byte_columns: Bool,
}

type GeneralNumericEntry = {key: Str, prefix: Str}
type SortRecord = {bytes: Bytes, text: Str?}
type SortRecordOrder = {record: SortRecord, key: Str}
type SortRecordMode = {month: Bool, human_numeric: Bool, general_numeric: Bool, version: Bool, numeric: Bool}
type SortKeyMode = {spec: Str, mode: SortRecordMode}
type SortRecordPlan = {has_key: Bool, mode: SortRecordMode, key_modes: List[SortKeyMode], plain_whole_line: Bool, last_resort: Bool}
type GeneralNumericInput = {text: Str, source_ends: List[Int]}
## `compressed` names the program that wrote a temporary run; null marks plain input or a plain run.
type SortInput = {name: Bytes, path: Path, stdin: Bool, compressed: Str?}
type SortMergeReader = {fd: Int, name: Bytes, pending: Bytes, pending_at: Int, eof: Bool, current: SortRecord?, current_order: Str?}
type SortKeyError = {kind: Str, value: Str}

## Unicode algorithms receive a view only when the original byte range is valid UTF-8.
pure sort_utf8_text(data: Bytes) -> Str? {
  if let Ok(value) = data.utf8() {
    value
  } else {
    null
  }
}

pure sort_record(data: Bytes) -> SortRecord {
  {bytes: data, text: sort_utf8_text(data)}
}

## Diagnostics use a byte quote when the input has no Unicode spelling.
## External sorts unwind their temp-root defer before returning shell status 130.
on SIGINT [] {
  exit 130
}

pure padded_decimal(value: Int, width: Int) -> Str {
  return f"{value:019}" when width == 19
  let digits = f"{value}"
  var padding = ""
  while padding.byte_len() + digits.byte_len() < width { padding += "0" }
  padding + digits
}

pure numeric_decimal_at(input: Bytes, at: Int, opts: SortOptions) -> Bool {
  let point = bytes.from_text(opts.decimal_point)
  at + point.len() <= input.len() and input[at..at + point.len()] == point
}

## A single-byte grouping mark may be invalid UTF-8 in the active locale.
pure numeric_prefix_raw(line: Bytes, opts: SortOptions) -> Bytes {
  var start = 0
  while start < line.len() and (line.byte_at(start) ?? 0) in [32, 9] { start += 1 }
  let value = line[start..]
  return b"" when value.starts_with(b"+")
  let sign_length = if value.starts_with(b"-") or value.starts_with(b"+") { 1 } else { 0 }
  var at = sign_length
  var digits = 0
  while at < value.len() {
    if is_ascii_digit(value.byte_at(at) ?? 0) {
      at += 1
      digits += 1
    } else if opts.thousands_separator.len() == 1 and value.byte_at(at) == opts.thousands_separator.byte_at(0) {
      at += 1
    } else {
      break
    }
  }
  if at < value.len() and numeric_decimal_at(value, at, opts) {
    at += opts.decimal_point.byte_len()
    while at < value.len() and is_ascii_digit(value.byte_at(at) ?? 0) {
      at += 1
      digits += 1
    }
  }
  if digits == 0 { b"" } else { value[0..at] }
}

## Normalize only the numeric prefix; suffixes and source offsets retain their bytes.
## GNU ignores a group separator when the locale encodes it with multiple bytes.
pure normalized_numeric_prefix_raw(prefix: Bytes, opts: SortOptions) -> Bytes {
  return prefix when opts.decimal_point == "." and opts.thousands_separator.len() != 1
  let point = bytes.from_text(opts.decimal_point)
  var at = 0
  let normalized = collect {
    while at < prefix.len() {
      if numeric_decimal_at(prefix, at, opts) {
        yield b"."
        at += point.len()
      } else if opts.thousands_separator.len() == 1 and prefix.byte_at(at) == opts.thousands_separator.byte_at(0) {
        at += 1
      } else {
        yield prefix[at..at + 1]
        at += 1
      }
    }
  }
  bytes.concat(normalized)
}

pure decimal_prefix(line: Str, opts: SortOptions) -> Str {
  let value = trim_leading_blanks(line)
  return "" when value.starts_with("+")
  let sign_length = if value.starts_with("-") or value.starts_with("+") { 1 } else { 0 }
  let input = bytes.from_text(value)
  var at = sign_length
  var digits = 0
  while at < input.len() {
    if is_ascii_digit(input.byte_at(at) ?? 0) {
      at += 1
      digits += 1
    } else if opts.thousands_separator.len() == 1 and input.byte_at(at) == opts.thousands_separator.byte_at(0) {
      at += 1
    } else {
      break
    }
  }
  if at < input.len() and numeric_decimal_at(input, at, opts) {
    at += opts.decimal_point.byte_len()
    while at < input.len() and is_ascii_digit(input.byte_at(at) ?? 0) {
      at += 1
      digits += 1
    }
  }
  return "" when digits == 0
  value.byte_slice(0, length: at)
}

pure human_numeric_prefix(line: Str, opts: SortOptions) -> Str {
  let value = trim_leading_blanks(line)
  let prefix = decimal_prefix(value, opts)
  return "" when prefix == ""
  let unit = value.byte_at(prefix.byte_len()) ?? 0
  if human_unit_order(unit) > 0 {
    prefix + value.byte_slice(prefix.byte_len(), length: 1)
  } else {
    prefix
  }
}

## GNU sort compares the suffix order before the decimal value, so 1000M sorts before 1G.
pure human_numeric_order_key_raw(line: Bytes, opts: SortOptions) -> Str {
  var start = 0
  while start < line.len() and (line.byte_at(start) ?? 0) in [32, 9] { start += 1 }
  let value = line[start..]
  let prefix = numeric_prefix_raw(value, opts)
  let number = numeric_order_key_raw(value, opts)
  let unit_byte = if prefix.is_empty() { 0 } else { value.byte_at(prefix.len()) ?? 0 }
  let unit = if number == "1" {
    0
  } else if prefix.starts_with(b"-") {
    0 - human_unit_order(unit_byte)
  } else {
    human_unit_order(unit_byte)
  }
  f"{padded_decimal(unit + 10, 2)}{number}"
}

pure human_unit_order(unit: Int) -> Int {
  return 1 when unit in [75, 107]
  return 2 when unit == 77
  return 3 when unit == 71
  return 4 when unit == 84
  return 5 when unit == 80
  return 6 when unit == 69
  return 7 when unit == 90
  return 8 when unit == 89
  return 9 when unit == 82
  return 10 when unit == 81
  0
}

## Month keys match the locale's complete abbreviated prefix after leading blanks.
## Unknown prefixes sort before recognized months.
pure month_order(line: Str, opts: SortOptions) -> Int {
  let value = trim_leading_blanks(line).upper()
  for index in range(opts.months.len()) {
    return index + 1 when value.starts_with(opts.months[index])
  }
  0
}

pure month_prefix(line: Str, opts: SortOptions) -> Str {
  let month = month_order(line, opts)
  return "" when month == 0
  trim_leading_blanks(line).byte_slice(0, length: opts.months[month - 1].byte_len())
}

pure invert_decimal_digits(digits: Str) -> Str {
  var inverted = ""
  for at in range(digits.byte_len()) {
    inverted += f"{9 - ((digits.byte_at(at) ?? 48) - 48)}"
  }
  inverted
}

pure numeric_exponent_key(exponent: Int) -> Str {
  if exponent < 0 {
    let magnitude = 0 - exponent
    "0" + invert_decimal_digits(padded_decimal(magnitude, 19))
  } else if exponent == 0 {
    "1" + padded_decimal(0, 19)
  } else {
    "2" + padded_decimal(exponent, 19)
  }
}

pure invert_numeric_exponent_key(key: Str) -> Str {
  var inverted = ""
  for at in range(key.byte_len()) {
    let byte = key.byte_at(at) ?? 48
    if at == 0 {
      inverted += if byte == 48 { "2" } else if byte == 50 { "0" } else { "1" }
    } else {
      inverted += f"{9 - (byte - 48)}"
    }
  }
  inverted
}

## Encode decimal magnitude and sign as text so numeric sorting never rounds or narrows the input.

pure numeric_order_key_raw(line: Bytes, opts: SortOptions) -> Str {
  let raw_prefix = numeric_prefix_raw(line, opts)
  return "1" when raw_prefix.is_empty()
  normalized_numeric_prefix_raw(raw_prefix, opts) |> numeric_order_key_normalized(_)
}

pure numeric_order_key_normalized(prefix: Bytes) -> Str {
  let negative = prefix.starts_with(b"-")
  let sign_length = if negative { 1 } else { 0 }
  var digits = ""
  var decimal_position = 0
  var after_decimal = false
  for at in range(sign_length, prefix.len()) {
    let byte = prefix.byte_at(at) ?? 0
    if byte == 46 {
      after_decimal = true
    } else {
      digits += f"{byte - 48}"
      if ! after_decimal { decimal_position += 1 }
    }
  }

  var first_significant = 0
  while first_significant < digits.byte_len() and digits.byte_slice(first_significant, length: 1) == "0" {
    first_significant += 1
  }
  return "1" when first_significant == digits.byte_len()
  let decimal_exponent = decimal_position - first_significant - 1
  var significant = digits.byte_slice(first_significant)
  while significant.ends_with("0") and significant.byte_len() > 1 {
    significant = significant.byte_slice(0, length: significant.byte_len() - 1)
  }

  if negative {
    "0" + invert_numeric_exponent_key(numeric_exponent_key(decimal_exponent)) + invert_decimal_digits(significant) + ":"
  } else {
    "2" + numeric_exponent_key(decimal_exponent) + significant + "!"
  }
}

pure has_general_numeric_keys(opts: SortOptions) -> Bool {
  return is_general_numeric_sort(opts) when opts.key.is_empty()
  for spec in opts.key {
    return true when is_general_numeric_sort(key_effective_options(spec, opts))
  }
  false
}

## Normalize only the locale decimal point into parser input; a non-UTF-8 byte
## after the numeric prefix terminates parsing without becoming a text alias.
proc general_numeric_input(value: Bytes, opts: SortOptions) [error] -> GeneralNumericInput {
  let decimal_point = bytes.from_text(opts.decimal_point)
  var text_value = ""
  var at = 0
  let source_ends = collect {
    while at < value.len() {
      if ! decimal_point.is_empty() and decimal_point.len() <= value.len() - at and
        value[at..at + decimal_point.len()] == decimal_point {
        text_value += "."
        yield at + decimal_point.len()
        at += decimal_point.len()
      } else {
        let byte = value.byte_at(at) ?? 0
        break when byte >= 128
        text_value += if opts.decimal_point != "." and byte == 46 { "/" } else { bytes.from_ints([byte])?.utf8()? }
        yield at + 1
        at += 1
      }
    }
  }
  {text: text_value, source_ends: source_ends}
}

## Parse once during input preparation, preserving the target's native long-double
## rounding and range. Keeping only requested strings bounds merge caches to the
## current records; a failed host conversion propagates before comparisons start.
proc prepare_general_numeric_keys(lines: List[SortRecord], opts: SortOptions) [env, error] -> SortOptions {
  let specs = collect {
    for spec in opts.key {
      yield spec when is_general_numeric_sort(key_effective_options(spec, opts))
    }
  }
  if (opts.key.is_empty() and ! is_general_numeric_sort(opts)) or
    (! opts.key.is_empty() and specs.is_empty()) { return opts }
  var keys: Map[GeneralNumericEntry] = {}
  for line in lines {
    let selected: List[Bytes] = if opts.key.is_empty() { [line.bytes] } else {
      [sort_record_key_bytes(line, spec, opts) for spec in specs]
    }
    for value in selected {
      let key = sort_direction_bytes(value, false)
      if key not in keys {
        if key in opts.general_numeric_keys {
          keys[key] = opts.general_numeric_keys[key]
        } else {
          let normalized = general_numeric_input(value, opts)
          let parsed = system.parse_long_double(normalized.text)?
          let consumed = if parsed.consumed == 0 { 0 } else { normalized.source_ends[parsed.consumed - 1] }
          var prefix_start = 0
          while prefix_start < value.len() and (value.byte_at(prefix_start) ?? 0) in [32, 9] { prefix_start += 1 }
          let prefix = if consumed <= prefix_start { "" } else if let Ok(text) = value[prefix_start..consumed].utf8() {
            text
          } else {
            normalized.text.byte_slice(prefix_start, length: parsed.consumed - prefix_start)
          }
          keys[key] = {key: parsed.order_key, prefix: prefix}
        }
      }
    }
  }
  {...opts, general_numeric_keys: keys}
}

## Comparisons consume prepared keys; a missing key is an input-preparation error.
## Failed conversions sort before NaNs, then native long-double values.

pure general_numeric_prefix(line: Str, opts: SortOptions) -> Str {
  opts.general_numeric_keys[sort_direction_bytes(bytes.from_text(line), false)].prefix
}

## Key positions have suffix modifiers, so read the leading decimal field number directly.
pure key_index(spec: Str) -> Int {
  let segment = spec.split(",").get(0) ?? "1"
  let input = bytes.from_text(segment)
  var end = 0
  while end < input.len() and is_ascii_digit(input.byte_at(end) ?? 0) { end += 1 }
  return 0 when end == 0
  (segment.byte_slice(0, length: end).parse_int() ?? 9223372036854775807) - 1
}

## Validate field and character positions before any key slicing can hide malformed input.
pure key_spec_error(spec: Str) -> SortKeyError? {
  let input = bytes.from_text(spec)
  var component_start = 0
  var component = 0
  loop {
    var component_end = component_start
    while component_end < input.len() and input.byte_at(component_end) != 44 { component_end += 1 }

    var at = component_start
    while at < component_end and is_ascii_digit(input.byte_at(at) ?? 0) { at += 1 }
    if at == component_start {
      return {kind: "field-start", value: spec} when component == 0
      return {kind: "comma-count", value: spec.byte_slice(component_start)}
    }

    let field_text = spec.byte_slice(component_start, length: at - component_start)
    let field = field_text.parse_int() ?? 9223372036854775807
    return {kind: "field-zero", value: spec} when field == 0

    if at < component_end and input.byte_at(at) == 46 {
      at += 1
      let count_start = at
      while at < component_end and is_ascii_digit(input.byte_at(at) ?? 0) { at += 1 }
      return {kind: "dot-count", value: spec.byte_slice(count_start)} when at == count_start
      let count_text = spec.byte_slice(count_start, length: at - count_start)
      let count = count_text.parse_int() ?? 9223372036854775807
      return {kind: "character-zero", value: spec} when count == 0
    }

    while at < component_end {
      let option = input.byte_at(at) ?? 0
      if option not in [98, 100, 102, 103, 104, 105, 77, 110, 82, 114, 86] {
        return {kind: "stray", value: spec}
      }
      at += 1
    }

    break when component_end == input.len()
    return {kind: "stray", value: spec} when component > 0
    component = 1
    component_start = component_end + 1
  }
  null
}

pure primary_key_spec(opts: SortOptions) -> Str {
  opts.key.get(0) ?? ""
}

## `\0` names the NUL byte as a separator; every other spelling is taken literally.
pure separator_value(raw: Str) -> Str {
  if raw == "\\0" { "\0" } else { raw }
}

## The last separator given wins; `main` rejects a set of separators that disagree.
pure sort_delimiter(opts: SortOptions) -> Str {
  if opts.delimiter.is_empty() { "" } else { separator_value(opts.delimiter[-1]) }
}

pure key_character_offset(spec: Str) -> Int {
  let start = (spec.split(",").get(0) ?? "").split(".").get(1) ?? ""
  let input = bytes.from_text(start)
  var end = 0
  while end < input.len() and is_ascii_digit(input.byte_at(end) ?? 0) { end += 1 }
  return 0 when end == 0
  let position = start.byte_slice(0, length: end).parse_int() ?? 9223372036854775807
  if position > 0 { position - 1 } else { 0 }
}

pure key_end_field_index(spec: Str) -> Int? {
  return null when spec.split(",").len() < 2
  key_index(spec.split(",").get(1) ?? "")
}

pure key_end_character_count(spec: Str) -> Int {
  let end = (spec.split(",").get(1) ?? "").split(".").get(1) ?? ""
  let input = bytes.from_text(end)
  var digits = 0
  while digits < input.len() and is_ascii_digit(input.byte_at(digits) ?? 0) { digits += 1 }
  return 0 when digits == 0
  let position = end.byte_slice(0, length: digits).parse_int() ?? 9223372036854775807
  if position > 0 { position } else { 0 }
}

pure key_reversed(spec: Str) -> Bool {
  "r" in key_flags(spec)
}

pure key_flags(spec: Str) -> Str {
  var flags = ""
  for segment in spec.split(",") {
    let input = bytes.from_text(segment)
    var at = 0
    while at < input.len() and is_ascii_digit(input.byte_at(at) ?? 0) { at += 1 }
    if at < input.len() and input.byte_at(at) == 46 {
      at += 1
      while at < input.len() and is_ascii_digit(input.byte_at(at) ?? 0) { at += 1 }
    }
    flags += segment.byte_slice(at)
  }
  flags
}

pure key_options_are_explicit(spec: Str) -> Bool {
  key_flags(spec) != ""
}

pure key_effective_options(spec: Str, opts: SortOptions) -> SortOptions {
  return opts unless key_options_are_explicit(spec)
  let flags = key_flags(spec)
  {
    ...opts,
    numeric: "n" in flags,
    human_numeric: "h" in flags,
    month: "M" in flags,
    general_numeric: "g" in flags,
    version_sort: "V" in flags,
    random: "R" in flags,
    sort_mode: [],
    fold_case: "f" in flags,
    dictionary: "d" in flags,
    ignore_nonprinting: "i" in flags,
    blank: opts.blank,
  }
}

## Names a -d or -i option that conflicts with a numeric-style mode, using the
## alphabetical letter order GNU sort reports conflicting option pairs in.
pure ordering_conflict(dictionary: Bool, mode: Str) -> Str {
  if ! dictionary and mode in ["g", "h"] { f"-{mode}i" } else { f"-{if dictionary { "d" } else { "i" }}{mode}" }
}

## Encode bytes with a low end marker so descending keys keep equal-key order.
pure sort_direction_bytes(input: Bytes, reverse: Bool) -> Str {
  var key = ""
  for index in range(input.len()) {
    let byte = input.byte_at(index) ?? 0
    let ordered = if reverse { 255 - byte } else { byte + 1 }
    key += padded_decimal(ordered, 3)
  }
  key + (if reverse { "256" } else { "000" })
}

pure sort_direction_key(value: Str, reverse: Bool) -> Str {
  sort_direction_bytes(bytes.from_text(value), reverse)
}

pure character_order_sort_key(input: Bytes, dictionary: Bool, ignore_nonprinting: Bool, fold_case: Bool, reverse: Bool) -> Str {
  var key = ""
  for index in range(input.len()) {
    let byte = input.byte_at(index) ?? 0
    let blank = byte == 9 or byte == 32
    let alphanumeric = (byte >= 48 and byte <= 57) or (byte >= 65 and byte <= 90) or (byte >= 97 and byte <= 122)
    let in_dictionary = ! dictionary or blank or alphanumeric
    let printable = byte >= 32 and byte <= 126
    let in_printable = ! ignore_nonprinting or printable
    if in_dictionary and in_printable {
      let folded = if fold_case and byte >= 97 and byte <= 122 { byte - 32 } else { byte }
      let ordered = if reverse { 255 - folded } else { folded + 1 }
      key += padded_decimal(ordered, 3)
    }
  }
  key + (if reverse { "256" } else { "000" })
}

## Numeric key parsers require valid text; offsets inside UTF-8 cannot start a numeric prefix.
pure key_text_boundary(line: Str, offset: Int) -> Bool {
  return true when offset <= 0 or offset >= line.byte_len()
  let byte = line.byte_at(offset) ?? 0
  byte < 128 or byte > 191
}

pure key_byte_field_count(line: Bytes, delimiter: Bytes) -> Int {
  guard delimiter.is_empty() else {
    var count = 1
    var at = 0
    while at + delimiter.len() <= line.len() {
      if line[at..at + delimiter.len()] == delimiter {
        count += 1
        at += delimiter.len()
      } else {
        at += 1
      }
    }
    return count
  }
  var count = 0
  var at = 0
  while at < line.len() {
    while at < line.len() and (line.byte_at(at) ?? 0) in [32, 9] { at += 1 }
    if at < line.len() {
      count += 1
      while at < line.len() and (line.byte_at(at) ?? 0) not in [32, 9] { at += 1 }
    }
  }
  count
}

pure key_byte_field_start(line: Bytes, delimiter: Bytes, field: Int) -> Int {
  guard delimiter.is_empty() else {
    var at = 0
    var current = 0
    while at + delimiter.len() <= line.len() {
      return at when current == field
      if line[at..at + delimiter.len()] == delimiter {
        at += delimiter.len()
        current += 1
      } else {
        at += 1
      }
    }
    return line.len()
  }
  var at = 0
  var current = 0
  while at < line.len() and current < field {
    while at < line.len() and (line.byte_at(at) ?? 0) in [32, 9] { at += 1 }
    while at < line.len() and (line.byte_at(at) ?? 0) not in [32, 9] { at += 1 }
    current += 1
  }
  at
}

pure key_byte_field_end(line: Bytes, delimiter: Bytes, field: Int) -> Int? {
  return null when field >= key_byte_field_count(line, delimiter)
  var at = key_byte_field_start(line, delimiter, field)
  if ! delimiter.is_empty() {
    while at + delimiter.len() <= line.len() {
      return at when line[at..at + delimiter.len()] == delimiter
      at += 1
    }
    return line.len()
  }
  while at < line.len() and (line.byte_at(at) ?? 0) in [32, 9] { at += 1 }
  while at < line.len() and (line.byte_at(at) ?? 0) not in [32, 9] { at += 1 }
  at
}

pure key_text_byte_boundary(line: Bytes, offset: Int, opts: SortOptions) -> Bool {
  return true when opts.debug_byte_columns or offset <= 0 or offset >= line.len()
  var at = 0
  while at < offset {
    let unit = text.character(line, at)
    return false when at + unit.size > offset
    at += unit.size
  }
  at == offset
}

pure key_start_byte_raw(line: Bytes, delimiter: Bytes, spec: Str, opts: SortOptions) -> Int {
  return line.len() when key_index(spec) >= key_byte_field_count(line, delimiter)
  var at = key_index(spec) |> key_byte_field_start(line, delimiter, _)
  let start_spec = spec.split(",").get(0) ?? ""
  if delimiter.is_empty() and (opts.blank or "b" in key_flags(start_spec) or is_numeric_sort(opts) or
    is_human_numeric_sort(opts) or is_general_numeric_sort(opts) or is_month_sort(opts)) {
    while at < line.len() and (line.byte_at(at) ?? 0) in [32, 9] { at += 1 }
  }
  let offset = key_character_offset(spec)
  if offset > line.len() - at { line.len() } else { at + offset }
}

pure key_range_end_byte_raw(line: Bytes, delimiter: Bytes, spec: Str, opts: SortOptions) -> Int? {
  let start_field = key_index(spec)
  if let end_field = key_end_field_index(spec) {
    return null when end_field < start_field
    if let field_end = key_byte_field_end(line, delimiter, end_field) {
      let end_count = key_end_character_count(spec)
      if end_count > 0 {
        var end_start = key_byte_field_start(line, delimiter, end_field)
        let end_spec = spec.split(",").get(1) ?? ""
        if delimiter.is_empty() and (opts.blank or "b" in key_flags(end_spec)) {
          while end_start < line.len() and (line.byte_at(end_start) ?? 0) in [32, 9] { end_start += 1 }
        }
        let remaining = field_end - end_start
        return if end_count < remaining { end_start + end_count } else { field_end }
      }
      return field_end
    }
    return null
  }
  line.len()
}

pure key_range_bytes_raw(line: Bytes, delimiter: Bytes, spec: Str, opts: SortOptions) -> Bytes {
  let start = key_start_byte_raw(line, delimiter, spec, opts)
  if let end = key_range_end_byte_raw(line, delimiter, spec, opts) {
    if end > start { line[start..end] } else { b"" }
  } else {
    b""
  }
}

pure key_field_value_raw(line: Bytes, delimiter: Bytes, spec: Str, opts: SortOptions) -> Bytes {
  let start = key_start_byte_raw(line, delimiter, spec, opts)
  return b"" unless key_text_byte_boundary(line, start, opts)
  if let end = key_range_end_byte_raw(line, delimiter, spec, opts) {
    var aligned_end = end
    while aligned_end > start and ! key_text_byte_boundary(line, aligned_end, opts) { aligned_end -= 1 }
    if aligned_end > start { line[start..aligned_end] } else { b"" }
  } else {
    b""
  }
}

pure key_field_value(line: Str, delimiter: Str, spec: Str, opts: SortOptions) -> Str {
  let start = key_start_byte(line, delimiter, spec, opts)
  return "" unless key_text_boundary(line, start)
  if let end = key_range_end_byte(line, delimiter, spec, opts) {
    var aligned_end = end
    while aligned_end > start and ! key_text_boundary(line, aligned_end) { aligned_end -= 1 }
    if aligned_end > start { line.byte_slice(start, length: aligned_end - start) } else { "" }
  } else {
    ""
  }
}

## Blank separators belong to a key boundary; a `b` on that boundary skips them only there.
pure key_range_end_byte(line: Str, delimiter: Str, spec: Str, opts: SortOptions) -> Int? {
  let start_field = key_index(spec)
  if let end_field = key_end_field_index(spec) {
    return null when end_field < start_field
    if let field_end = key_field_end_byte(line, delimiter, end_field) {
      let end_count = key_end_character_count(spec)
      if end_count > 0 {
        var end_start = key_field_start_byte(line, delimiter, end_field)
        let end_spec = spec.split(",").get(1) ?? ""
        if opts.blank or "b" in key_flags(end_spec) {
          let input = bytes.from_text(line)
          while end_start < input.len() and (input.byte_at(end_start) ?? 0) in [32, 9] { end_start += 1 }
        }
        let remaining = field_end - end_start
        return if end_count < remaining { end_start + end_count } else { field_end }
      }
      return field_end
    }
    return null
  }
  line.byte_len()
}

pure key_range_bytes(line: Str, delimiter: Str, spec: Str, opts: SortOptions) -> Bytes {
  let input = bytes.from_text(line)
  let start = key_start_byte(line, delimiter, spec, opts)
  if let end = key_range_end_byte(line, delimiter, spec, opts) {
    if end > start { input[start..end] } else { b"" }
  } else {
    b""
  }
}

pure key_field_start_byte(line: Str, delimiter: Str, field: Int) -> Int {
  guard delimiter == "" else {
    let parts = line.split(delimiter)
    return line.byte_len() when field >= parts.len()
    var position = 0
    for index in range(field) { position += parts[index].byte_len() + delimiter.byte_len() }
    return position
  }

  let input = bytes.from_text(line)
  var position = 0
  var current_field = 0
  while position < input.len() and current_field < field {
    while position < input.len() and (input.byte_at(position) ?? 0) in [32, 9] { position += 1 }
    while position < input.len() and (input.byte_at(position) ?? 0) not in [32, 9] { position += 1 }
    current_field += 1
  }
  position
}

pure key_field_end_byte(line: Str, delimiter: Str, field: Int) -> Int? {
  guard delimiter == "" else {
    let parts = line.split(delimiter)
    return null when field >= parts.len()
    return key_field_start_byte(line, delimiter, field) + parts[field].byte_len()
  }

  let input = bytes.from_text(line)
  return null when field >= line.trim().words().len()
  var position = key_field_start_byte(line, delimiter, field)
  while position < input.len() and (input.byte_at(position) ?? 0) in [32, 9] { position += 1 }
  while position < input.len() and (input.byte_at(position) ?? 0) not in [32, 9] { position += 1 }
  position
}

pure key_start_byte(line: Str, delimiter: Str, spec: Str, opts: SortOptions) -> Int {
  if key_index(spec) >= (if delimiter == "" { line.trim().words().len() } else { line.split(delimiter).len() }) { return line.byte_len() }
  let input = bytes.from_text(line)
  var position = key_index(spec) |> key_field_start_byte(line, delimiter, _)
  let start_spec = spec.split(",").get(0) ?? ""
  if opts.blank or "b" in key_flags(start_spec) or is_numeric_sort(opts) or is_human_numeric_sort(opts) or
    is_general_numeric_sort(opts) or is_month_sort(opts) {
    while position < input.len() and (input.byte_at(position) ?? 0) in [32, 9] { position += 1 }
  }
  let offset = key_character_offset(spec)
  if offset > input.len() - position { input.len() } else { position + offset }
}

## The debug overlay ignores NUL bytes when it aligns columns, because NUL bytes are often
## removed from the output before it is inspected.
pure non_nul_byte_count(input: Bytes) -> Int {
  var count = 0
  for at in range(input.len()) {
    if (input.byte_at(at) ?? 0) != 0 { count += 1 }
  }
  count
}

## Debug tabs are displayed as one marker; other decoded characters use the
## shared renderer's terminal width. Byte locales count every non-NUL byte.
pure debug_column_count(input: Bytes, opts: SortOptions) -> Int {
  return non_nul_byte_count(input) when opts.debug_byte_columns
  var columns = 0
  var at = 0
  while at < input.len() {
    if input.byte_at(at) == 9 {
      columns += 1
      at += 1
    } else {
      let character = text.character(input, at)
      columns += character.width
      at += character.size
    }
  }
  columns
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

# Sort key whose plain order is GNU's filenvercmp order (used by sort -V). Operands are split like
# filevercmp's file suffixes: the prefix before a trailing `.alpha...` run is compared first, and the
# whole name breaks ties. A leading dot ranks "." before ".." before other dot names, then ordinary names.
pure version_key_bytes(input: Bytes) -> Str {
  return "A" when input.is_empty()
  return "B" when input == b"."
  return "C" when input == b".."
  let class = if input.byte_at(0) == 46 { "D" } else { "E" }
  let prefix = version_prefix_length(input)
  class + version_revision_key_bytes(input[0..prefix]) + version_revision_key_bytes(input)
}

# The length of the name without its suffix, following filenvercmp's forward scan: a suffix is a
# run of `.` followed by a letter or `~` and then alphanumerics or `~`.
pure version_prefix_length(input: Bytes) -> Int {
  let count = input.len()
  var at = 0
  var prefix = 0
  while at < count {
    at += 1
    prefix = at
    while at + 1 < count and input.byte_at(at) == 46 and (is_ascii_letter(input.byte_at(at + 1) ?? 0) or input.byte_at(at + 1) == 126) {
      at += 2
      while at < count and (is_ascii_digit(input.byte_at(at) ?? 0) or is_ascii_letter(input.byte_at(at) ?? 0) or input.byte_at(at) == 126) { at += 1 }
    }
  }
  prefix
}

# Encodes a name as tokens whose plain order matches filenvercmp's element order: `~` < a zero run
# followed by `~` < end < a zero run followed by another byte < digit run < letter < other byte.
# filenvercmp skips a zero run that meets the end of the other name, so a zero run at the end is
# dropped and an interior one is ranked by the byte after it. A digit run compares by its
# significant digits, length first, so leading zeros and the run's own length keep numeric order.
pure version_revision_key_bytes(input: Bytes) -> Str {
  var key = ""
  var at = 0
  while at < input.len() {
    let byte = input.byte_at(at) ?? 0
    if is_ascii_digit(byte) {
      var end = at + 1
      while end < input.len() and is_ascii_digit(input.byte_at(end) ?? 0) { end += 1 }

      var significant = at
      while significant < end and input.byte_at(significant) == 48 { significant += 1 }
      if significant == end {
        if end < input.len() {
          key += if input.byte_at(end) == 126 { "1a" } else { "2a" }
        }
      } else {
        var digits = ""
        for digit_at in range(significant, end) { digits += f"{(input.byte_at(digit_at) ?? 48) - 48}" }
        key += "3" + padded_decimal(end - significant, 20) + digits
      }
      at = end
    } else if byte == 126 {
      key += "0"
      at += 1
    } else {
      key += if is_ascii_letter(byte) { "4" } else { "5" }
      key += version_hex_byte(byte)
      at += 1
    }
  }
  key + "2"
}

pure selected_sort_mode(opts: SortOptions) -> Str {
  opts.sort_mode.get(opts.sort_mode.len() - 1) ?? ""
}

pure is_month_mode(mode: Str) -> Bool {
  mode != "" and "month".starts_with(mode)
}

pure is_general_numeric_mode(mode: Str) -> Bool {
  mode != "" and "general-numeric".starts_with(mode)
}

pure is_human_numeric_mode(mode: Str) -> Bool {
  mode != "" and (mode in ["h", "human"] or "human-numeric".starts_with(mode))
}

pure is_version_sort_mode(opts: SortOptions, mode: Str) -> Bool {
  opts.version_sort or mode.starts_with("v")
}

pure is_month_sort_mode(opts: SortOptions, mode: Str) -> Bool {
  if mode == "" { opts.month } else { is_month_mode(mode) }
}

pure is_general_numeric_sort_mode(opts: SortOptions, mode: Str) -> Bool {
  if mode == "" { opts.general_numeric } else { is_general_numeric_mode(mode) }
}

pure is_numeric_sort_mode(opts: SortOptions, mode: Str) -> Bool {
  if mode == "" { opts.numeric } else { mode in ["n", "numeric"] }
}

pure is_human_numeric_sort_mode(opts: SortOptions, mode: Str) -> Bool {
  if mode == "" { opts.human_numeric } else { is_human_numeric_mode(mode) }
}

pure sort_record_mode(opts: SortOptions) -> SortRecordMode {
  let mode = selected_sort_mode(opts)
  {
    month: is_month_sort_mode(opts, mode),
    human_numeric: is_human_numeric_sort_mode(opts, mode),
    general_numeric: is_general_numeric_sort_mode(opts, mode),
    version: is_version_sort_mode(opts, mode),
    numeric: is_numeric_sort_mode(opts, mode),
  }
}

pure is_version_sort(opts: SortOptions) -> Bool {
  is_version_sort_mode(opts, selected_sort_mode(opts))
}

pure is_month_sort(opts: SortOptions) -> Bool {
  is_month_sort_mode(opts, selected_sort_mode(opts))
}

pure is_general_numeric_sort(opts: SortOptions) -> Bool {
  is_general_numeric_sort_mode(opts, selected_sort_mode(opts))
}

pure is_numeric_sort(opts: SortOptions) -> Bool {
  is_numeric_sort_mode(opts, selected_sort_mode(opts))
}

pure is_human_numeric_sort(opts: SortOptions) -> Bool {
  is_human_numeric_sort_mode(opts, selected_sort_mode(opts))
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

pure trim_leading_blanks(line: Str) -> Str {
  let characters = line.split("")
  var at = 0
  while at < characters.len() and characters[at] in [" ", "\t"] { at += 1 }
  characters |> drop(at) |> join("")
}

pure sort_record_key_bytes(record: SortRecord, spec: Str, opts: SortOptions) -> Bytes {
  key_field_value_raw(record.bytes, bytes.from_text(sort_delimiter(opts)), spec, key_effective_options(spec, opts))
}

pure sort_record_key_value(record: SortRecord, spec: Str, opts: SortOptions, mode: SortRecordMode) -> Str {
  let key_opts = key_effective_options(spec, opts)
  let reverse = opts.reverse != key_reversed(spec)
  let selected = key_range_bytes_raw(record.bytes, bytes.from_text(sort_delimiter(opts)), spec, key_opts)
  if ! mode.month and ! mode.human_numeric and ! mode.general_numeric and ! mode.version and ! mode.numeric {
    return character_order_sort_key(selected, key_opts.dictionary, key_opts.ignore_nonprinting, key_opts.fold_case, reverse)
  }
  let key = if mode.month {
    padded_decimal(if let Ok(value) = selected.utf8() { month_order(value, opts) } else { 0 }, 2)
  } else if mode.human_numeric {
    human_numeric_order_key_raw(selected, key_opts)
  } else if mode.general_numeric {
    opts.general_numeric_keys[sort_direction_bytes(selected, false)].key
  } else if mode.version {
    version_key_bytes(selected)
  } else {
    numeric_order_key_raw(selected, key_opts)
  }
  if reverse { sort_direction_key(key, true) } else { key }
}

pure sort_record_primary_keys(record: SortRecord, opts: SortOptions, plan: SortRecordPlan) -> List[Str] {
  if plan.has_key {
    let keys = collect {
      for key in plan.key_modes { yield sort_record_key_value(record, key.spec, opts, key.mode) }
    }
    return keys
  }
  if plan.mode.month {
    let key = padded_decimal(if let value = record.text { month_order(value, opts) } else { 0 }, 2)
    return [if opts.reverse { sort_direction_key(key, true) } else { key }]
  }
  if plan.mode.human_numeric {
    let key = human_numeric_order_key_raw(record.bytes, opts)
    return [if opts.reverse { sort_direction_key(key, true) } else { key }]
  }
  if plan.mode.general_numeric {
    let key = opts.general_numeric_keys[sort_direction_bytes(record.bytes, false)].key
    return [if opts.reverse { sort_direction_key(key, true) } else { key }]
  }
  if plan.mode.version {
    let key = version_key_bytes(record.bytes)
    return [if opts.reverse { sort_direction_key(key, true) } else { key }]
  }
  if plan.mode.numeric {
    let key = numeric_order_key_raw(record.bytes, opts)
    return [if opts.reverse { sort_direction_key(key, true) } else { key }]
  }
  if opts.blank {
    var start = 0
    while start < record.bytes.len() and (record.bytes.byte_at(start) ?? 0) in [32, 9] { start += 1 }
    return [character_order_sort_key(record.bytes[start..], opts.dictionary, opts.ignore_nonprinting, opts.fold_case, opts.reverse)]
  }
  if opts.fold_case or opts.dictionary or opts.ignore_nonprinting {
    return [character_order_sort_key(record.bytes, opts.dictionary, opts.ignore_nonprinting, opts.fold_case, opts.reverse)]
  }
  [sort_direction_bytes(record.bytes, opts.reverse)]
}

## GNU compares keys in order, then uses the raw line unless stable or unique was requested.
pure sort_record_order(record: SortRecord, opts: SortOptions, plan: SortRecordPlan) -> Str {
  var order = ""
  for key in sort_record_primary_keys(record, opts, plan) { order += key + "\0" }
  if plan.last_resort { order += sort_direction_bytes(record.bytes, opts.reverse) }
  order
}

pure sort_record_plan(opts: SortOptions, has_key: Bool) -> SortRecordPlan {
  let mode = sort_record_mode(opts)
  let key_modes: List[SortKeyMode] = collect {
    if has_key {
      for spec in opts.key {
        yield {spec: spec, mode: sort_record_mode(key_effective_options(spec, opts))}
      }
    }
  }
  let special_order = mode.month or mode.human_numeric or mode.general_numeric or mode.version or mode.numeric
  let plain_line_key = ! has_key and ! special_order and ! opts.blank and ! opts.fold_case and ! opts.dictionary and
    ! opts.ignore_nonprinting
  {
    has_key: has_key,
    mode: mode,
    key_modes: key_modes,
    plain_whole_line: ! has_key and ! opts.random and ! special_order and ! opts.blank and ! opts.fold_case and
      ! opts.dictionary and ! opts.ignore_nonprinting,
    last_resort: ! opts.stable and ! opts.unique and ! plain_line_key,
  }
}

pure sort_records(records: List[SortRecord], opts: SortOptions, plan: SortRecordPlan) -> List[SortRecord] {
  let unique_input = if opts.unique { records |> unique-by . } else { records }
  return unique_input when opts.merge
  ## UTF-8 string order matches raw byte order only when every record is valid UTF-8.
  if plan.plain_whole_line {
    var all_text = true
    let ordered: List[SortRecordOrder] = collect {
      for record in unique_input {
        if let text = record.text {
          yield {record: record, key: text}
        } else {
          all_text = false
          break
        }
      }
    }
    if all_text {
      return [entry.record for entry in ordered |> sort-by(desc: opts.reverse) .key]
    }
  }
  let ordered: List[SortRecordOrder] = collect {
    for record in unique_input { yield {record: record, key: sort_record_order(record, opts, plan)} }
  }
  [entry.record for entry in ordered |> sort-by .key]
}

pure same_sort_record_key(left: SortRecord, right: SortRecord, opts: SortOptions, plan: SortRecordPlan) -> Bool {
  sort_record_primary_keys(left, opts, plan) == sort_record_primary_keys(right, opts, plan)
}

pure unique_sorted_records(records: List[SortRecord], opts: SortOptions, plan: SortRecordPlan) -> List[SortRecord] {
  return records unless opts.unique
  var unique: List[SortRecord] = []
  for record in records {
    if unique.is_empty() or ! same_sort_record_key(unique[-1], record, opts, plan) {
      unique += [record]
    }
  }
  unique
}

pure sort_record_precedes(left: SortRecord, right: SortRecord, opts: SortOptions, plan: SortRecordPlan) -> Bool {
  sort_record_order(left, opts, plan) < sort_record_order(right, opts, plan)
}

pure sort_merge_reader_order(reader: SortMergeReader, opts: SortOptions, plan: SortRecordPlan) -> SortMergeReader {
  if let current = reader.current {
    {...reader, current_order: sort_record_order(current, opts, plan)}
  } else {
    {...reader, current_order: null}
  }
}

pure sort_record_bytes(records: List[SortRecord], ending: Bytes) -> Bytes {
  let output = collect {
    for record in records { yield @[record.bytes, ending] }
  }
  bytes.concat(output)
}

pure input_records(input: Bytes, zero_terminated: Bool) -> List[SortRecord] {
  let separator = if zero_terminated { 0 } else { 10 }
  return [] when input.is_empty()
  if let Ok(text) = input.utf8() {
    let delimiter = if zero_terminated { "\0" } else { "\n" }
    let parts = text.split(delimiter)
    let count = parts.len() - (if input.byte_at(input.len() - 1) == separator { 1 } else { 0 })
    return [{bytes: bytes.from_text(parts[index]), text: parts[index]} for index in range(count)]
  }
  var start = 0
  var at = 0
  let records: List[SortRecord] = collect {
    while at < input.len() {
      if input.byte_at(at) == separator {
        yield sort_record(input[start..at])
        start = at + 1
      }
      at += 1
    }
    yield sort_record(input[start..]) when start < input.len()
  }
  records
}

## Split before decoding so POSIX filenames with invalid UTF-8 remain usable paths.
pure files0_names(data: Bytes) -> List[Bytes] {
  var start = 0
  let names = collect {
    for at in range(data.len()) {
      if data.byte_at(at) == 0 {
        yield data[start..at]
        start = at + 1
      }
    }
    yield data[start..] when start < data.len()
  }
  names
}

pure debug_primary_text(line: Str, opts: SortOptions, has_key: Bool) -> Str {
  if has_key {
    let selected = key_field_value(line, sort_delimiter(opts), primary_key_spec(opts), opts)
    if is_month_sort(opts) {
      month_prefix(selected, opts)
    } else if is_human_numeric_sort(opts) {
      human_numeric_prefix(selected, opts)
    } else if is_numeric_sort(opts) {
      decimal_prefix(selected, opts)
    } else {
      selected
    }
  } else if is_month_sort(opts) {
    month_prefix(line, opts)
  } else if is_numeric_sort(opts) {
    decimal_prefix(line, opts)
  } else if is_human_numeric_sort(opts) {
    human_numeric_prefix(line, opts)
  } else if is_general_numeric_sort(opts) {
    general_numeric_prefix(line, opts)
  } else if opts.blank {
    trim_leading_blanks(line)
  } else {
    line
  }
}

pure debug_key_text(line: Str, opts: SortOptions, spec: Str) -> Str {
  let key_opts = key_effective_options(spec, opts)
  let selected = sort_delimiter(opts) |> key_field_value(line, _, spec, key_opts)
  if is_month_sort(key_opts) {
    month_prefix(selected, opts)
  } else if is_human_numeric_sort(key_opts) {
    human_numeric_prefix(selected, opts)
  } else if is_numeric_sort(key_opts) {
    decimal_prefix(selected, opts)
  } else if is_general_numeric_sort(key_opts) {
    general_numeric_prefix(selected, key_opts)
  } else {
    selected
  }
}

pure debug_visible_line(line: Str) -> Str {
  line.replace("\t", with: ">")
}

pure leading_blank_count(line: Str) -> Int {
  let input = bytes.from_text(line)
  var at = 0
  while at < input.len() and (input.byte_at(at) == 32 or input.byte_at(at) == 9) { at += 1 }
  at
}

pure debug_annotation(value: Str, opts: SortOptions) -> Str {
  if value == "" { "^ no match for key" } else { text.padding(debug_column_count(bytes.from_text(value), opts), "_") }
}

pure debug_key_annotation(line: Str, opts: SortOptions, spec: Str) -> Str {
  let key_opts = key_effective_options(spec, opts)
  if is_month_sort(key_opts) or is_human_numeric_sort(key_opts) or is_numeric_sort(key_opts) or is_general_numeric_sort(key_opts) {
    debug_key_text(line, opts, spec) |> debug_annotation(_, opts)
  } else {
    let range = sort_delimiter(opts) |> key_range_bytes(line, _, spec, key_opts)
    if range.is_empty() { "^ no match for key" } else { text.padding(debug_column_count(range, opts), "_") }
  }
}

pure debug_sort_text(lines: List[Str], opts: SortOptions, has_key: Bool) -> Str {
  let has_last_resort = has_key or is_month_sort(opts) or is_numeric_sort(opts) or is_human_numeric_sort(opts) or is_general_numeric_sort(opts) or is_version_sort(opts) or opts.blank or opts.fold_case or opts.dictionary or opts.ignore_nonprinting
  var output = ""
  for line in lines {
    output += debug_visible_line(line) + "\n"
    if has_key {
      for spec in opts.key {
        let key_opts = key_effective_options(spec, opts)
        let start = sort_delimiter(opts) |> key_start_byte(line, _, spec, key_opts)
        let selected = sort_delimiter(opts) |> key_field_value(line, _, spec, key_opts)
        let numeric_blanks = if is_month_sort(key_opts) or is_human_numeric_sort(key_opts) or is_numeric_sort(key_opts) or is_general_numeric_sort(key_opts) { leading_blank_count(selected) } else { 0 }
        let indentation = text.padding(debug_column_count(bytes.from_text(line)[..start], opts) + numeric_blanks, " ")
        output += indentation + debug_key_annotation(line, opts, spec) + "\n"
      }
      if ! opts.stable and ! opts.unique { output += debug_annotation(line, opts) + "\n" }
    } else {
      let primary = debug_primary_text(line, opts, false)
      let indentation = if is_month_sort(opts) or is_general_numeric_sort(opts) or is_numeric_sort(opts) or is_human_numeric_sort(opts) or opts.blank { text.padding(leading_blank_count(line), " ") } else { "" }
      output += indentation + debug_annotation(primary, opts) + "\n"
      if has_last_resort and ! opts.stable and ! opts.unique { output += debug_annotation(line, opts) + "\n" }
    }
  }
  output
}

pure debug_visible_bytes(line: Bytes) -> Bytes {
  let visible = collect {
    for at in range(line.len()) {
      yield if line.byte_at(at) == 9 { b">" } else { line[at..at + 1] }
    }
  }
  bytes.concat(visible)
}

pure trim_leading_blanks_raw(line: Bytes) -> Bytes {
  var start = 0
  while start < line.len() and (line.byte_at(start) ?? 0) in [32, 9] { start += 1 }
  line[start..]
}

pure leading_blank_count_raw(line: Bytes) -> Int {
  var at = 0
  while at < line.len() and (line.byte_at(at) ?? 0) in [32, 9] { at += 1 }
  at
}

pure human_numeric_prefix_raw(line: Bytes, opts: SortOptions) -> Bytes {
  let value = trim_leading_blanks_raw(line)
  let prefix = numeric_prefix_raw(value, opts)
  return b"" when prefix.is_empty()
  let unit = value.byte_at(prefix.len()) ?? 0
  if human_unit_order(unit) > 0 { bytes.concat([prefix, value[prefix.len()..prefix.len() + 1]]) } else { prefix }
}

pure debug_record_primary_bytes(record: SortRecord, opts: SortOptions, has_key: Bool) -> Bytes {
  if has_key {
    let selected = primary_key_spec(opts) |> sort_record_key_bytes(record, _, opts)
    let key_opts = key_effective_options(primary_key_spec(opts), opts)
    if is_month_sort(key_opts) {
      if let Ok(value) = selected.utf8() { return bytes.from_text(month_prefix(value, opts)) }
      return b""
    }
    return human_numeric_prefix_raw(selected, key_opts) when is_human_numeric_sort(key_opts)
    return numeric_prefix_raw(selected, key_opts) when is_numeric_sort(key_opts)
    if is_general_numeric_sort(key_opts) {
      return bytes.from_text(opts.general_numeric_keys[sort_direction_bytes(selected, false)].prefix)
    }
    return selected
  }
  if is_month_sort(opts) {
    if let value = record.text { return bytes.from_text(month_prefix(value, opts)) }
    return b""
  }
  return human_numeric_prefix_raw(record.bytes, opts) when is_human_numeric_sort(opts)
  return numeric_prefix_raw(record.bytes, opts) when is_numeric_sort(opts)
  if is_general_numeric_sort(opts) {
    return bytes.from_text(opts.general_numeric_keys[sort_direction_bytes(record.bytes, false)].prefix)
  }
  return trim_leading_blanks_raw(record.bytes) when opts.blank
  record.bytes
}

pure debug_key_annotation_bytes(record: SortRecord, opts: SortOptions, spec: Str) -> Str {
  let key_opts = key_effective_options(spec, opts)
  if is_month_sort(key_opts) or is_human_numeric_sort(key_opts) or is_numeric_sort(key_opts) or is_general_numeric_sort(key_opts) {
    let selected = sort_record_key_bytes(record, spec, opts)
    let value = if is_month_sort(key_opts) {
      if let Ok(text) = selected.utf8() { bytes.from_text(month_prefix(text, opts)) } else { b"" }
    } else if is_human_numeric_sort(key_opts) {
      human_numeric_prefix_raw(selected, key_opts)
    } else if is_numeric_sort(key_opts) {
      numeric_prefix_raw(selected, key_opts)
    } else {
      bytes.from_text(opts.general_numeric_keys[sort_direction_bytes(selected, false)].prefix)
    }
    if value.is_empty() { "^ no match for key" } else { text.padding(debug_column_count(value, opts), "_") }
  } else {
    let range = sort_record_key_bytes(record, spec, opts)
    if range.is_empty() { "^ no match for key" } else { text.padding(debug_column_count(range, opts), "_") }
  }
}

pure debug_sort_records(records: List[SortRecord], opts: SortOptions, has_key: Bool) -> Bytes {
  let has_last_resort = has_key or is_month_sort(opts) or is_numeric_sort(opts) or is_human_numeric_sort(opts) or is_general_numeric_sort(opts) or is_version_sort(opts) or opts.blank or opts.fold_case or opts.dictionary or opts.ignore_nonprinting
  let output = collect {
    for record in records {
      if let line = record.text {
        yield bytes.from_text(debug_sort_text([line], opts, has_key))
      } else {
        yield @[debug_visible_bytes(record.bytes), b"\n"]
        if has_key {
          for spec in opts.key {
            let key_opts = key_effective_options(spec, opts)
            let start = key_start_byte_raw(record.bytes, bytes.from_text(sort_delimiter(opts)), spec, key_opts)
            let selected = sort_record_key_bytes(record, spec, opts)
            let numeric_blanks = if is_month_sort(key_opts) or is_human_numeric_sort(key_opts) or is_numeric_sort(
              key_opts,
            ) or is_general_numeric_sort(key_opts) {
              leading_blank_count_raw(selected)
            } else {
              0
            }
            yield @[
              bytes.from_text(text.padding(debug_column_count(record.bytes[0..start], opts) + numeric_blanks, " ")),
              bytes.from_text(debug_key_annotation_bytes(record, opts, spec)),
              b"\n",
            ]
          }

          if ! opts.stable and ! opts.unique {
            yield @[bytes.from_text(debug_annotation_bytes(record.bytes, opts)), b"\n"]
          }
        } else {
          let primary = debug_record_primary_bytes(record, opts, false)
          let indentation = if is_month_sort(opts) or is_general_numeric_sort(opts) or is_numeric_sort(opts) or is_human_numeric_sort(
            opts,
          ) or opts.blank {
            leading_blank_count_raw(record.bytes)
          } else {
            0
          }
          yield @[
            bytes.from_text(text.padding(indentation, " ")),
            bytes.from_text(debug_annotation_bytes(primary, opts)),
            b"\n",
          ]
          if has_last_resort and ! opts.stable and ! opts.unique {
            yield @[bytes.from_text(debug_annotation_bytes(record.bytes, opts)), b"\n"]
          }
        }
      }
    }
  }
  bytes.concat(output)
}

pure debug_annotation_bytes(value: Bytes, opts: SortOptions) -> Str {
  if value.is_empty() { "^ no match for key" } else { text.padding(debug_column_count(value, opts), "_") }
}

pure has_random_key(opts: SortOptions) -> Bool {
  for spec in opts.key {
    return true when "R" in key_flags(spec)
  }
  false
}

## Random order compares an MD5 digest of the salt and the key, so equal keys get
## equal digests and stay adjacent for -u. Only the first key is hashed.
pure random_sort_records(records: List[SortRecord], opts: SortOptions, has_key: Bool, salt: Bytes) -> List[SortRecord] {
  records |> sort-by(desc: opts.reverse) { |record| {
    digest: {
      let selected = if has_key { primary_key_spec(opts) |> sort_record_key_bytes(record, _, opts) } else { record.bytes }
      let key = if let Ok(value) = selected.utf8() {
        bytes.from_text(character_order_key(value, opts.dictionary, opts.ignore_nonprinting, opts.fold_case))
      } else {
        selected
      }
      bytes.concat([salt, key]).md5().hex()
    },
    raw: if opts.stable { "" } else { sort_direction_bytes(record.bytes, false) },
  }}
}

## GNU reads one 16-byte salt per run; a shorter regular-file source is an error
## rather than a weaker salt.
proc random_salt(source: Str?) [fs, process, env, error] -> Bytes {
  let name = source ?? "/dev/urandom"
  let source_path = fp"{name}"
  match fs.stat(source_path, follow_symlinks: true) {
    Ok(metadata) => {
      if metadata.mode / 4096 % 16 == 8 and metadata.size < 16 {
        gnu.error(f"{gnu.quote(name)}: end of file")
        exit 2
      }
    }
    Err(failure) => {
      gnu.error(f"{gnu.quote(name)}: {gnu.strerror(failure)}")
      exit 2
    }
  }
  match bytes.read_at(source_path, 0, 16) {
    Ok(salt) => salt
    Err(failure) => {
      gnu.error(f"{gnu.quote(name)}: {gnu.strerror(failure)}")
      exit 2
    }
  }
}

## A required output operand may start with a dash, so attach it before parsing options.
pure normalize_output_args(argv: List[Str]) -> List[Str] {
  var normalized = []
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
    } else if ! operands_only and arg == "--check" {
      # A bare --check takes no value; the argument parser would otherwise
      # consume the following operand as the check mode.
      normalized += ["--check=diagnose-first"]
      at += 1
    } else {
      normalized += [arg]
      at += 1
    }
  }
  normalized
}

type LegacyKeyPart = {field: Str, char_pos: Str, opts: Str}

## Decimal count without leading zeros, saturated at the largest unsigned value so
## that oversized legacy positions behave as GNU's unsigned parse does.
pure unsigned_count(digits: Str) -> Str {
  let largest = "18446744073709551615"
  var start = 0
  while start < digits.byte_len() - 1 and digits.byte_slice(start, length: 1) == "0" { start += 1 }
  let trimmed = digits.byte_slice(start)
  if trimmed.byte_len() > largest.byte_len() or (trimmed.byte_len() == largest.byte_len() and trimmed > largest) {
    largest
  } else {
    trimmed
  }
}

pure unsigned_successor(digits: Str) -> Str {
  let count = unsigned_count(digits)
  return count when count == "18446744073709551615"
  var sum = ""
  var carry = 1
  var at = count.byte_len()
  while at > 0 {
    at -= 1
    let digit = (count.byte_at(at) ?? 48) - 48 + carry
    sum = f"{digit % 10}{sum}"
    carry = digit / 10
  }
  if carry == 1 { "1" + sum } else { sum }
}

## `POS1` or `POS1.CHAR` followed by option letters, as in the traditional `+POS1 [-POS2]` form.
pure legacy_key_part(spec: Str) -> LegacyKeyPart? {
  let input = bytes.from_text(spec)
  var at = 0
  while at < input.len() and is_ascii_digit(input.byte_at(at) ?? 0) { at += 1 }
  return null when at == 0
  var char_pos = "0"
  var rest = spec.byte_slice(at)
  if rest.starts_with(".") {
    let tail = bytes.from_text(rest)
    var stop = 1
    while stop < tail.len() and is_ascii_digit(tail.byte_at(stop) ?? 0) { stop += 1 }
    return null when stop == 1
    char_pos = unsigned_count(rest.byte_slice(1, length: stop - 1))
    rest = rest.byte_slice(stop)
  }
  {field: unsigned_count(spec.byte_slice(0, length: at)), char_pos: char_pos, opts: rest}
}

## The `-k` spelling of a traditional key: fields are one-based, and an end position
## with no character offset stays on its field, clamped to field 1.
pure legacy_key_spec(from: LegacyKeyPart, to: LegacyKeyPart?) -> Str {
  var keydef = unsigned_successor(from.field)
  if from.char_pos != "0" { keydef += f".{unsigned_successor(from.char_pos)}" }
  keydef += from.opts
  if let end = to {
    let end_field = if end.char_pos != "0" {
      unsigned_successor(end.field)
    } else if end.field == "0" {
      "1"
    } else {
      end.field
    }
    keydef += f",{end_field}"
    if end.char_pos != "0" { keydef += f".{end.char_pos}" }
    keydef += end.opts
  }
  keydef
}

## GNU keeps the traditional `+POS1 [-POS2]` form unless _POSIX2_VERSION is in
## [200112, 200809); an unset or non-numeric value keeps the form.
proc traditional_key_syntax() [env] -> Bool {
  if let Ok(version) = (env.get_or("_POSIX2_VERSION", "") ?? "").parse_int() {
    version < 200112 or version >= 200809
  } else {
    true
  }
}

pure legacy_key_args(argv: List[Str], traditional: Bool) -> List[Str] {
  return argv unless traditional
  var at = 0
  let normalized = collect {
    while at < argv.len() {
      let arg = argv[at]
      if arg == "--" {
        for index in range(at, argv.len()) { yield argv[index] }
        break
      }

      let from: LegacyKeyPart? = if arg.starts_with("+") { legacy_key_part(arg.byte_slice(1)) } else { null }
      if let start = from {
        var to: LegacyKeyPart? = null
        let next_is_position = at + 1 < argv.len() and argv[at + 1].starts_with("-") and argv[at + 1].byte_len() > 1 and is_ascii_digit(
          bytes.from_text(argv[at + 1]).byte_at(1) ?? 0,
        )
        if next_is_position {
          if let parsed = legacy_key_part(argv[at + 1].byte_slice(1)) {
            to = parsed
            at += 1
          } else {
            yield @[arg, argv[at + 1]]
            at += 2
            continue
          }
        }

        yield f"-k{legacy_key_spec(start, to)}"
        at += 1
      } else {
        yield arg
        at += 1
      }
    }
  }
  normalized
}

## A final unterminated record in one operand ends before the next operand starts.
proc read_sort_records(sources: List[SortInput], zero_terminated: Bool) [fs, io, process, error] -> List[List[SortRecord]] {
  let records: List[List[SortRecord]] = collect {
    for source in sources {
      let part = if source.stdin {
        io.stdin_bytes()?
      } else {
        match source.path.read_bytes() {
          Ok(data) => data
          Err(failure) => {
            gnu.error(f"cannot read: {gnu.quote_bytes(source.name, always: false)}: {gnu.strerror(failure)}")
            exit 2
          }
        }
      }
      yield input_records(part, zero_terminated)
    }
  }
  records
}

## Merge one current record from each input; ties keep the earlier input first.
pure merge_sort_records(sources: List[List[SortRecord]], opts: SortOptions, has_key: Bool) -> List[SortRecord] {
  var positions = [0 for _ in sources]
  let plan = sort_record_plan(opts, has_key)
  let merged: List[SortRecord] = collect {
    loop {
      var selected = null
      for index in range(sources.len()) {
        let position = positions[index]
        if position < sources[index].len() {
          if let current_index = selected {
            let candidate = sources[index][position]
            let current = sources[current_index][positions[current_index]]
            if sort_record_precedes(candidate, current, opts, plan) {
              selected = index
            }
          } else {
            selected = index
          }
        }
      }

      if let index = selected {
        yield sources[index][positions[index]]
        positions[index] += 1
      } else {
        break
      }
    }
  }

  merged
}

## Read one newline or NUL-delimited record while retaining only the unfinished suffix.
proc advance_sort_merge_reader(reader: SortMergeReader, zero_terminated: Bool) [io, error] -> Result[SortMergeReader, Error] {
  var pending = reader.pending
  var pending_at = reader.pending_at
  var eof = reader.eof
  let separator = if zero_terminated { 0 } else { 10 }

  loop {
    for at in range(pending_at, pending.len()) {
      if pending.byte_at(at) == separator {
        let record = sort_record(pending[pending_at..at])
        return Ok({...reader, pending: pending, pending_at: at + 1, eof: eof, current: record, current_order: null})
      }
    }

    if eof {
      if pending_at >= pending.len() {
        return Ok({...reader, pending: b"", pending_at: 0, current: null, current_order: null, eof: true})
      }
      let record = sort_record(pending[pending_at..])
      return Ok({...reader, pending: b"", pending_at: 0, current: record, current_order: null, eof: true})
    }

    let chunk = unix.read_fd(reader.fd, 65536)?
    if chunk.is_empty() {
      eof = true
    } else {
      pending = bytes.concat([pending[pending_at..], chunk])
      pending_at = 0
    }
  }
}

## Open all source streams only when their descriptor count stays within the process and batch limits.
proc can_stream_sort_merge(inputs: List[SortInput], opts: SortOptions, check_enabled: Bool, inputs_are_regular: Bool) [process, error] -> Bool {
  if ! opts.merge or check_enabled or opts.debug or ! opts.output.is_empty() or ! inputs_are_regular { return false }

  let limit = process.rlimit("nofile")?
  let open_limit = if let soft = limit.soft { soft - 3 } else { inputs.len() }
  let batch_limit = if let size = opts.batch_size { size as Int } else { open_limit }
  let maximum = if batch_limit < open_limit { batch_limit } else { open_limit }
  inputs.len() <= maximum
}

## Merge sorted file streams directly to stdout so a failed writer stops further reads.
proc stream_sort_merge(inputs: List[SortInput], opts: SortOptions, has_key: Bool) [fs, io, env, process, error] {
  let plan = sort_record_plan(opts, has_key)
  var readers: List[SortMergeReader] = []
  for input in inputs {
    match unix.open_fd(input.path) {
      Ok(fd) => readers += [{fd: fd, name: input.name, pending: b"", pending_at: 0, eof: false, current: null, current_order: null}]
      Err(failure) => {
        gnu.error(f"cannot read: {gnu.quote_bytes(input.name, always: false)}: {gnu.strerror(failure)}")
        exit 2
      }
    }
  }

  for index in range(readers.len()) {
    match advance_sort_merge_reader(readers[index], opts.zero_terminated) {
      Ok(reader) => readers[index] = if has_general_numeric_keys(opts) { reader } else { sort_merge_reader_order(reader, opts, plan) }
      Err(failure) => {
        gnu.error(f"cannot read: {gnu.quote_bytes(readers[index].name, always: false)}: {gnu.strerror(failure)}")
        exit 2
      }
    }
  }

  let line_ending = if opts.zero_terminated { b"\0" } else { b"\n" }
  var output = []
  var output_bytes = 0
  var previous: SortRecord? = null
  var prepared = opts
  loop {
    if has_general_numeric_keys(opts) {
      let active: List[SortRecord] = collect {
        for reader in readers { if let line = reader.current { yield line } }
        if let line = previous { yield line }
      }
      prepared = prepare_general_numeric_keys(active, prepared)
      for index in range(readers.len()) { readers[index] = sort_merge_reader_order(readers[index], prepared, plan) }
    }
    var selected = null
    for index in range(readers.len()) {
      if let _ = readers[index].current {
        if let current_index = selected {
          let candidate_order = readers[index].current_order ?? ""
          let current_order = readers[current_index].current_order ?? ""
          if candidate_order < current_order { selected = index }
        } else {
          selected = index
        }
      }
    }

    if let index = selected {
      let line = readers[index].current ?? sort_record(b"")
      let duplicate = if opts.unique {
        if let prior = previous { same_sort_record_key(prior, line, prepared, plan) } else { false }
      } else { false }
      if ! duplicate {
        output += [line.bytes, line_ending]
        output_bytes += line.bytes.len() + line_ending.len()
        if output_bytes >= 65536 {
          finish_sort_stdout_bytes(bytes.concat(output), intermediate: true)
          output = []
          output_bytes = 0
        }
      }
      previous = line
      match advance_sort_merge_reader(readers[index], opts.zero_terminated) {
        Ok(reader) => readers[index] = if has_general_numeric_keys(opts) { reader } else { sort_merge_reader_order(reader, prepared, plan) }
        Err(failure) => {
          gnu.error(f"cannot read: {gnu.quote_bytes(readers[index].name, always: false)}: {gnu.strerror(failure)}")
          exit 2
        }
      }
    } else {
      break
    }
  }

  if ! output.is_empty() { finish_sort_stdout_bytes(bytes.concat(output)) }
  for reader in readers { unix.close_fd(reader.fd) }
}

## GNU sort reserves three descriptors for standard streams and signal handling.
proc validate_batch_size(value: Str) [process, env, error] -> Unit {
  let raw = bytes.from_text(value)
  var valid = ! raw.is_empty()
  for at in range(raw.len()) {
    if ! is_ascii_digit(raw.byte_at(at) ?? 0) { valid = false }
  }
  if ! valid {
    gnu.error(f"invalid --batch-size argument {gnu.quote(value)}")
    exit 2
  }

  let limit = process.rlimit("nofile")?
  let maximum = if let soft = limit.soft { soft - 3 } else { 9223372036854775807 }
  if let Ok(size) = value.parse_int() {
    guard size >= 2 else {
      gnu.error(f"invalid --batch-size argument {gnu.quote(value)}")
      gnu.error("minimum --batch-size argument is '2'")
      exit 2
    }
    if size > maximum {
      gnu.error(f"--batch-size argument {gnu.quote(value)} too large")
      gnu.error(f"maximum --batch-size argument with current rlimit is {maximum}")
      exit 2
    }
  } else {
    gnu.error(f"--batch-size argument {gnu.quote(value)} too large")
    gnu.error(f"maximum --batch-size argument with current rlimit is {maximum}")
    exit 2
  }
}

## The count is parsed the way GNU's strtoumax-based check does: leading blanks and
## one '+' may precede the digits, a '-' is rejected, and any trailing text is a
## suffix error. The value only sizes the thread pool, so a valid count is accepted
## without changing the output. The value is quoted with plain quotes, as GNU does.
proc validate_parallel(value: Str) [process] -> Unit {
  let raw = bytes.from_text(value)
  var start = 0
  while start < raw.len() and (raw.byte_at(start) ?? 0) in [32, 9, 10, 11, 12, 13] { start += 1 }
  if start < raw.len() and raw.byte_at(start) == 43 { start += 1 }
  var digits = start
  while digits < raw.len() and is_ascii_digit(raw.byte_at(digits) ?? 0) { digits += 1 }
  if digits == start {
    gnu.error(f"invalid --parallel argument '{value}'")
    exit 2
  }
  if digits != raw.len() {
    gnu.error(f"invalid suffix in --parallel argument '{value}'")
    exit 2
  }
  var zero = true
  for at in range(start, digits) {
    if (raw.byte_at(at) ?? 0) != 48 { zero = false }
  }
  if zero {
    gnu.error("number in parallel must be nonzero")
    exit 2
  }
}

proc parse_buffer_size(value: Str) [fs, env, process, error] -> Int {
  let raw = bytes.from_text(value)
  # Leading white space is skipped, as strtoumax does for the size operand.
  var start = 0
  while start < raw.len() and (raw.byte_at(start) ?? 0) in [32, 9, 10, 11, 12, 13] { start += 1 }
  var digits = start
  while digits < raw.len() and is_ascii_digit(raw.byte_at(digits) ?? 0) { digits += 1 }
  if digits == start {
    gnu.error(f"invalid -S argument {gnu.quote(value)}")
    exit 2
  }

  let number_text = value.byte_slice(start, length: digits - start)
  let suffix = value.byte_slice(digits)
  if suffix != "%" and suffix.find("%") != null {
    gnu.error(f"invalid suffix in -S argument {gnu.quote(value)}")
    exit 2
  }
  if suffix == "%" {
    if let Err(_) = number_text.parse_uint() {
      gnu.error(f"invalid suffix in -S argument {gnu.quote(value)}")
      exit 2
    }
  }
  let number = number_text.parse_int() ?? { |_|
    gnu.error(f"-S argument {gnu.quote(value)} too large")
    exit 2
  }
  if suffix == "%" {
    let memory = linux.meminfo()?
    if number > 100 {
      gnu.error(f"-S argument {gnu.quote(value)} too large")
      exit 2
    }
    return memory.total / 100 * number + memory.total % 100 * number / 100
  }

  let power = if suffix in ["", "b"] { 0 } else if suffix in ["k", "K"] { 1 } else if suffix == "M" { 2 } else if suffix == "G" { 3 } else if suffix == "T" { 4 } else if suffix == "P" { 5 } else if suffix == "E" { 6 } else if suffix == "Z" { 7 } else if suffix == "Y" { 8 } else {
    gnu.error(f"invalid suffix in -S argument {gnu.quote(value)}")
    exit 2
  }

  let maximum = 9223372036854775807
  var multiplier = 1
  repeat power times {
    if multiplier > maximum / 1024 {
      gnu.error(f"-S argument {gnu.quote(value)} too large")
      exit 2
    }
    multiplier *= 1024
  }
  if number > maximum / multiplier {
    gnu.error(f"-S argument {gnu.quote(value)} too large")
    exit 2
  }
  number * multiplier
}

## Flush an intermediate merge chunk so failed output stops further input reads.
## The runtime buffers stdout, so an intermediate chunk's flush still belongs
## to the write phase; only the final flush uses the fflush diagnostic. A closed
## reader ends the process with the SIGPIPE status and no diagnostic.
proc finish_sort_stdout_bytes(data: Bytes, intermediate = false) [io, env, process] -> Unit {
  if let Err(failure) = io.write_stdout_bytes(data) {
    if gnu.errno(failure) == 32 { exit 141 }
    gnu.error(f"write failed: 'standard output': {gnu.strerror(failure)}")
    gnu.error("write error")
    exit 2
  }
  if let Err(failure) = io.flush_stdout() {
    if gnu.errno(failure) == 32 { exit 141 }
    let action = if intermediate { "write failed" } else { "fflush failed" }
    gnu.error(f"{action}: 'standard output': {gnu.strerror(failure)}")
    gnu.error("write error")
    exit 2
  }
}

proc write_sort_fd(fd: Int, data: Bytes, output_name: Str) [io, env, process, error] -> Unit {
  var offset = 0
  while offset < data.len() {
    match unix.write_fd(fd, data[offset..]) {
      Ok(written) => {
        if written == 0 {
          gnu.error(f"write failed: {gnu.quote(output_name)}: descriptor write made no progress")
          exit 2
        }
        offset += written
      }
      Err(failure) => {
        gnu.error(f"write failed: {gnu.quote(output_name)}: {gnu.strerror(failure)}")
        exit 2
      }
    }
  }
}

proc create_sort_temp_root(directory: Str?) [fs, io, process, env, error] -> FsRoot {
  let created = if let temp_dir = directory { fs.tempdir_in(fp"{temp_dir}") } else { fs.tempdir() }
  match created {
    Ok(root) => root
    Err(failure) => {
      # GNU names the directory it tried, falling back to $TMPDIR and then /tmp.
      let location = if let temp_dir = directory { temp_dir } else { if let Ok(value) = env.get("TMPDIR") { value } else { "/tmp" } }
      gnu.error(f"cannot create temporary file in {gnu.quote(location)}: {gnu.strerror(failure)}")
      exit 2
    }
  }
}

proc sort_temp_path(root: FsRoot, name: Str) [fs, error] -> Path {
  let relative = fp"{name}"
  root.write(relative, b"")
  root.chmod(relative, 0o600)
  fp"{root.host_path()?}/{name}"
}

## GNU sort reports a compression program it cannot start and then sorts without compressing temporary runs.
proc startable_compress_program(program: Str?) [process, env] -> Str? {
  if let name = program {
    match process.which(name) {
      Ok(_) => return name
      Err(failure) => {
        let reason = if failure is NotFound { "No such file or directory" } else { gnu.strerror(failure) }
        gnu.error(f"could not run compress program {gnu.quote(name)}: {reason}")
        return null
      }
    }
  }
  null
}

## Runs the compression program with the bytes as stdin and the output file as stdout.
## A program that does not exit 0 leaves a truncated run behind, so it stops the sort.
proc run_compress_program(program: Str, argv: List[Str], input: Bytes, output: Path) [process, env, error] {
  let status = process.run(process.command_argv(program, argv, stdin: input, stdout: output)) ?? { |failure|
    gnu.error(f"could not run compress program {gnu.quote(program)}: {gnu.strerror(failure)}")
    exit 2
  }
  if ! status.exited_with(0) {
    gnu.error(f"{gnu.quote(program)} terminated abnormally")
    exit 2
  }
}

proc write_temp_run(root: FsRoot, name: Str, contents: Bytes, compress: Str?) [fs, process, env, error] -> SortInput {
  let run_path = sort_temp_path(root, name)
  if let program = compress {
    run_compress_program(program, [program], contents, run_path)
  } else {
    root.write(fp"{name}", contents)
    root.chmod(fp"{name}", 0o600)
  }
  {name: run_path.bytes(), path: run_path, stdin: false, compressed: compress}
}

## A plain file that was merged into can become a run only after compression; the plain copy is then removed.
proc compress_temp_run(root: FsRoot, name: Str, plain: Path, compress: Str?) [fs, process, env, error] -> SortInput {
  if compress != null {
    let stored = write_temp_run(root, name, plain.read_bytes()?, compress)
    plain.remove()
    stored
  } else {
    {name: plain.bytes(), path: plain, stdin: false, compressed: null}
  }
}

## Returns the input as plain text, decompressing a temporary run into a new file under the root.
proc plain_sort_input(root: FsRoot, input: SortInput, name: Str) [fs, process, env, error] -> SortInput {
  if let program = input.compressed {
    let plain = sort_temp_path(root, name)
    run_compress_program(program, [program, "-d"], input.path.read_bytes()?, plain)
    {name: plain.bytes(), path: plain, stdin: false, compressed: null}
  } else {
    input
  }
}

proc plain_sort_inputs(root: FsRoot, inputs: List[SortInput], prefix: Str) [fs, process, env, error] -> List[SortInput] {
  let plain: List[SortInput] = collect {
    for index in range(inputs.len()) {
      yield plain_sort_input(root, inputs[index], f"{prefix}-in-{index}")
    }
  }
  plain
}

proc write_sort_run(root: FsRoot, name: Str, records: List[SortRecord], opts: SortOptions, has_key: Bool, compress: Str?) [fs, process, env, error] -> SortInput {
  let prepared = prepare_general_numeric_keys(records, opts)
  let plan = sort_record_plan(prepared, has_key)
  let sorted = unique_sorted_records(sort_records(records, prepared, plan), prepared, plan)
  let ending = if opts.zero_terminated { b"\0" } else { b"\n" }
  write_temp_run(root, name, sort_record_bytes(sorted, ending), compress)
}

# A fan-in below two cannot reduce the number of runs, so the batch limit never drops under two.
proc sort_merge_batch_limit(opts: SortOptions) [process, error] -> Int {
  let limit = process.rlimit("nofile")?
  let available = if let soft = limit.soft { if soft > 8 { soft - 6 } else { 2 } } else { 32 }
  let bounded = if available > 64 { 64 } else { available }
  if let requested = opts.batch_size {
    let size = requested as Int
    if size < bounded { size } else { bounded }
  } else {
    bounded
  }
}

proc merge_sort_stream(inputs: List[SortInput], opts: SortOptions, has_key: Bool, output_fd: Int?, deduplicate: Bool) [fs, io, env, process, error] {
  let plan = sort_record_plan(opts, has_key)
  var readers: List[SortMergeReader] = []
  defer {
    for reader in readers {
      if reader.fd != 0 { let _ = unix.close_fd(reader.fd) }
    }
  }

  for input in inputs {
    let fd = if input.stdin {
      0
    } else {
      match unix.open_fd(input.path) {
        Ok(fd) => fd
        Err(failure) => {
          gnu.error(f"cannot read: {gnu.quote_bytes(input.name, always: false)}: {gnu.strerror(failure)}")
          exit 2
        }
      }
    }
    readers += [{fd: fd, name: input.name, pending: b"", pending_at: 0, eof: false, current: null, current_order: null}]
  }

  for index in range(readers.len()) {
    match advance_sort_merge_reader(readers[index], opts.zero_terminated) {
      Ok(reader) => readers[index] = if has_general_numeric_keys(opts) { reader } else { sort_merge_reader_order(reader, opts, plan) }
      Err(failure) => {
        gnu.error(f"cannot read: {gnu.quote_bytes(readers[index].name, always: false)}: {gnu.strerror(failure)}")
        exit 2
      }
    }
  }

  let line_ending = if opts.zero_terminated { b"\0" } else { b"\n" }
  var output = []
  var output_bytes = 0
  var previous: SortRecord? = null
  var prepared = opts
  loop {
    if has_general_numeric_keys(opts) {
      let active: List[SortRecord] = collect {
        for reader in readers { if let line = reader.current { yield line } }
        if let line = previous { yield line }
      }
      prepared = prepare_general_numeric_keys(active, prepared)
      for index in range(readers.len()) { readers[index] = sort_merge_reader_order(readers[index], prepared, plan) }
    }
    var selected = null
    for index in range(readers.len()) {
      if let _ = readers[index].current {
        if let current_index = selected {
          let candidate_order = readers[index].current_order ?? ""
          let current_order = readers[current_index].current_order ?? ""
          if candidate_order < current_order { selected = index }
        } else {
          selected = index
        }
      }
    }

    if let index = selected {
      let line = readers[index].current ?? sort_record(b"")
      let duplicate = if deduplicate {
        if let prior = previous { same_sort_record_key(prior, line, prepared, plan) } else { false }
      } else { false }
      if ! duplicate {
        output += [line.bytes, line_ending]
        output_bytes += line.bytes.len() + line_ending.len()
        if output_bytes >= 65536 {
          let output_chunk = bytes.concat(output)
          if let fd = output_fd { write_sort_fd(fd, output_chunk, "temporary file") } else { finish_sort_stdout_bytes(output_chunk, intermediate: true) }
          output = []
          output_bytes = 0
        }
      }
      previous = line
      match advance_sort_merge_reader(readers[index], opts.zero_terminated) {
        Ok(reader) => readers[index] = if has_general_numeric_keys(opts) { reader } else { sort_merge_reader_order(reader, prepared, plan) }
        Err(failure) => {
          gnu.error(f"cannot read: {gnu.quote_bytes(readers[index].name, always: false)}: {gnu.strerror(failure)}")
          exit 2
        }
      }
    } else {
      break
    }
  }

  if ! output.is_empty() {
    let output_chunk = bytes.concat(output)
    if let fd = output_fd { write_sort_fd(fd, output_chunk, "temporary file") } else { finish_sort_stdout_bytes(output_chunk) }
  }
}

proc merge_sort_batched(inputs: List[SortInput], root: FsRoot, opts: SortOptions, has_key: Bool, batch_limit: Int, output_fd: Int?, deduplicate: Bool, compress: Str?) [fs, io, env, process, error] {
  var runs = inputs
  var pass = 0
  while runs.len() > batch_limit {
    var next: List[SortInput] = []
    var start = 0
    var group_number = 0
    while start < runs.len() {
      let stop = if start + batch_limit < runs.len() { start + batch_limit } else { runs.len() }
      let input_group: List[SortInput] = collect {
        for index in range(start, stop) { yield runs[index] }
      }
      if input_group.len() == 1 {
        next += input_group
      } else {
        let name = f"merge-{pass}-{group_number}"
        let plain_inputs = plain_sort_inputs(root, input_group, name)
        let merged = sort_temp_path(root, f"{name}-plain")
        let fd = unix.open_fd(merged, write: true)?
        merge_sort_stream(plain_inputs, opts, has_key, fd, false)
        unix.close_fd(fd)
        next += [compress_temp_run(root, name, merged, compress)]
      }
      start = stop
      group_number += 1
    }
    runs = next
    pass += 1
  }
  let plain_runs = plain_sort_inputs(root, runs, f"final-{pass}")
  merge_sort_stream(plain_runs, opts, has_key, output_fd, deduplicate)
}

proc buffered_sort(inputs: List[SortInput], opts: SortOptions, buffer_size: Int, has_key: Bool, output: Path, has_output: Bool) [fs, io, env, process, error] {
  let compress = startable_compress_program(opts.compress_program)
  var temp_root = null
  defer { if let root = temp_root { root.close() } }

  let requested_limit = if buffer_size == 0 { 9223372036854775807 } else { buffer_size }
  ## A minimum run size prevents tiny -S values from creating a temporary file for every short record.
  let chunk_limit = if requested_limit < 65536 { 65536 } else { requested_limit }
  let separator_bytes = if opts.zero_terminated { 1 } else { 1 }
  var chunk: List[SortRecord] = []
  var chunk_bytes = 0
  var runs: List[SortInput] = []
  var run_number = 0

  for input in inputs {
    let fd = if input.stdin {
      0
    } else {
      match unix.open_fd(input.path) {
        Ok(fd) => fd
        Err(failure) => {
          gnu.error(f"cannot read: {gnu.quote_bytes(input.name, always: false)}: {gnu.strerror(failure)}")
          exit 2
        }
      }
    }
    var reader: SortMergeReader = SortMergeReader(fd:, name: input.name, pending: b"", pending_at: 0, eof: false, current: null, current_order: null)
    loop {
      match advance_sort_merge_reader(reader, opts.zero_terminated) {
        Ok(next) => {
          reader = next
          if let line = reader.current {
            chunk += [line]
            chunk_bytes += line.bytes.len() + separator_bytes
            if chunk_bytes >= chunk_limit {
              if temp_root == null { temp_root = create_sort_temp_root(opts.temp_directory) }
              if let root = temp_root {
                runs += [write_sort_run(root, f"run-{run_number}", chunk, opts, has_key, compress)]
              } else {
                fail "sort temporary directory was not created"
              }
              run_number += 1
              chunk = []
              chunk_bytes = 0
            }
          } else {
            break
          }
        }
        Err(failure) => {
          gnu.error(f"cannot read: {gnu.quote_bytes(input.name, always: false)}: {gnu.strerror(failure)}")
          exit 2
        }
      }
    }
    if ! input.stdin { unix.close_fd(fd) }
  }

  if runs.is_empty() {
    let prepared = prepare_general_numeric_keys(chunk, opts)
    let plan = sort_record_plan(prepared, has_key)
    let lines = unique_sorted_records(sort_records(chunk, prepared, plan), prepared, plan)
    let line_ending = if opts.zero_terminated { b"\0" } else { b"\n" }
    let output_data = sort_record_bytes(lines, line_ending)
    if has_output {
      if let Err(failure) = output.write(output_data) {
        let action = if let Ok(_) = fs.stat(output, follow_symlinks: true) { "write failed" } else { "open failed" }
        gnu.error(f"{action}: {gnu.quote_maybe(output.display())}: {gnu.strerror(failure)}")
        exit 2
      }
    } else {
      finish_sort_stdout_bytes(output_data)
    }
    return
  }

  if ! chunk.is_empty() {
    if let root = temp_root {
      runs += [write_sort_run(root, f"run-{run_number}", chunk, opts, has_key, compress)]
    } else {
      fail "sort temporary directory was not created"
    }
  }
  if let root = temp_root {
    let batch_limit = sort_merge_batch_limit(opts)
    if has_output {
      if let Err(failure) = output.write(b"") {
        let action = if let Ok(_) = fs.stat(output, follow_symlinks: true) { "write failed" } else { "open failed" }
        gnu.error(f"{action}: {gnu.quote_maybe(output.display())}: {gnu.strerror(failure)}")
        exit 2
      }
      let fd = unix.open_fd(output, write: true)?
      merge_sort_batched(runs, root, opts, has_key, batch_limit, fd, opts.unique, compress)
      unix.close_fd(fd)
    } else {
      merge_sort_batched(runs, root, opts, has_key, batch_limit, null, opts.unique, compress)
    }
  } else {
    fail "sort temporary directory was not created"
  }
}

proc sort_locale_name(category: Str) [env, error] -> Str {
  let all = env.get_or("LC_ALL", "")?
  return all when all != ""
  let selected = env.get_or(category, "")?
  return selected when selected != ""
  let language = env.get_or("LANG", "")?
  if language != "" { language } else { "C" }
}

pure sort_needs_numeric_conventions(opts: SortOptions) -> Bool {
  if is_numeric_sort(opts) or is_general_numeric_sort(opts) or is_human_numeric_sort(opts) { return true }
  for spec in opts.key {
    let flags = key_flags(spec)
    return true when "n" in flags or "g" in flags or "h" in flags
  }
  false
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let parsed_args = cli.applet(
    normalize_output_args(legacy_key_args(argv, traditional_key_syntax())),
    {
      gnu: {status: 2},
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
      human_numeric: {
        form: "-h --human-numeric-sort",
        default: false,
      },
      month: {
        form: "-M --month-sort",
        default: false,
      },
      general_numeric: {
        form: "-g --general-numeric-sort",
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
      buffer_size: {
        form: "-S --buffer-size SIZE",
      },
      debug: {
        form: "--debug",
        default: false,
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
      merge: {
        form: "-m --merge",
        default: false,
      },
      batch_size: {
        form: "--batch-size SIZE",
      },
      temp_directory: {
        form: "-T --temporary-directory DIR",
      },
      compress_program: {
        form: "--compress-program PROG",
      },
      key: {
        form: "-k --key KEY",
        repeated: true,
      },
      delimiter: {
        form: "-t --field-separator DELIMITER",
        repeated: true,
      },
      parallel: {
        form: "--parallel N",
        repeated: true,
      },
      output: {
        form: "-o --output FILE",
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
      files0_from: {
        form: "--files0-from FILE",
      },
      help: {
        form: "--help",
        default: false,
        stop: true,
      },
      version: {
        form: "--version",
        default: false,
        stop: true,
      },
      random: {
        form: "-R --random-sort",
        default: false,
      },
      random_source: {
        form: "--random-source FILE",
      },
      paths: {
        form: "...FILE",
      },
    },
  )?

  if parsed_args.help {
    gnu.help(USAGE)
    return
  }
  if parsed_args.version {
    gnu.version("sort")
    return
  }

  var opts = SortOptions(
    ...parsed_args,
    general_numeric_keys: {},
    decimal_point: ".",
    thousands_separator: b"",
    months: [],
    debug_byte_columns: text.byte_columns(),
  )
  if sort_needs_numeric_conventions(opts) {
    let conventions = system.locale_numeric_info(sort_locale_name("LC_NUMERIC"))?
    opts = {...opts, decimal_point: conventions.decimal_point as Str, thousands_separator: conventions.thousands_separator}
  }
  var needs_months = is_month_sort(opts)
  for spec in opts.key {
    if is_month_sort(key_effective_options(spec, opts)) { needs_months = true }
  }
  if needs_months {
    let calendar = system.locale_time_info(sort_locale_name("LC_TIME"))?
    let months = collect {
      for name in calendar.abbreviated_months { yield name.utf8()?.upper() }
    }
    opts = {...opts, months: months}
  }

  for count in opts.parallel { validate_parallel(count) }

  for spec in opts.key {
    if let issue = key_spec_error(spec) {
      let message = if issue.kind == "field-start" {
        f"invalid number at field start: invalid field specification {gnu.quote(issue.value)}"
      } else if issue.kind == "field-zero" {
        f"field number is zero: invalid field specification {gnu.quote(issue.value)}"
      } else if issue.kind == "character-zero" {
        f"character offset is zero: invalid field specification {gnu.quote(issue.value)}"
      } else if issue.kind == "dot-count" {
        f"invalid number after '.': invalid count at start of {gnu.quote(issue.value)}"
      } else if issue.kind == "comma-count" {
        f"invalid number after ',': invalid count at start of {gnu.quote(issue.value)}"
      } else {
        f"stray character in field spec: invalid field specification {gnu.quote(issue.value)}"
      }
      gnu.error(message)
      exit 2
    }
  }

  # A key's own ordering letters are checked here; global -d and -i are not, because
  # a key that names its own ordering does not inherit them.
  for spec in opts.key {
    if key_options_are_explicit(spec) {
      let key_opts = key_effective_options(spec, opts)
      let key_mode = if key_opts.numeric { "n" } else if key_opts.human_numeric { "h" } else if key_opts.general_numeric { "g" } else if key_opts.month { "M" } else { "" }
      if key_mode != "" and (key_opts.dictionary or key_opts.ignore_nonprinting) {
        gnu.error(f"options '{ordering_conflict(key_opts.dictionary, key_mode)}' are incompatible")
        exit 2
      }
    }
  }

  let buffer_size: Int? = if let value = opts.buffer_size { parse_buffer_size(value) } else { null }
  if let batch_size = opts.batch_size { validate_batch_size(batch_size) }

  let selected_mode = selected_sort_mode(opts)
  let general_numeric = is_general_numeric_sort(opts)
  let human_numeric = is_human_numeric_sort(opts)
  if selected_mode != "" and ! selected_mode.starts_with("v") and ! is_human_numeric_mode(selected_mode) and ! is_month_mode(selected_mode) and ! is_general_numeric_mode(selected_mode) and selected_mode not in ["n", "numeric", "random"] {
    gnu.error(f"invalid argument {gnu.quote_maybe(selected_mode)} for '--sort'")
    exit 2
  }

  let numeric_requested = opts.numeric or is_numeric_sort(opts)
  let general_numeric_requested = opts.general_numeric or general_numeric
  let human_numeric_requested = opts.human_numeric or human_numeric
  let month_requested = opts.month or is_month_sort(opts)
  let random_requested = opts.random or selected_mode == "random" or has_random_key(opts)
  if random_requested and (general_numeric_requested or human_numeric_requested or month_requested or numeric_requested) {
    # Letters follow GNU's order; the ignore-nonprinting flag is not part of this list.
    var letters = ""
    if opts.dictionary { letters += "d" }
    if opts.fold_case { letters += "f" }
    if general_numeric_requested { letters += "g" }
    if human_numeric_requested { letters += "h" }
    if month_requested { letters += "M" }
    if numeric_requested { letters += "n" }
    letters += "R"
    gnu.error(f"options '-{letters}' are incompatible")
    exit 2
  }
  if numeric_requested and general_numeric_requested {
    gnu.error("options '-gn' are incompatible")
    exit 2
  }
  if human_numeric_requested and general_numeric_requested {
    gnu.error("options '-gh' are incompatible")
    exit 2
  }
  if human_numeric_requested and numeric_requested {
    gnu.error("options '-hn' are incompatible")
    exit 2
  }
  if month_requested and general_numeric_requested {
    gnu.error("options '-gM' are incompatible")
    exit 2
  }
  if month_requested and human_numeric_requested {
    gnu.error("options '-hM' are incompatible")
    exit 2
  }
  if month_requested and numeric_requested {
    gnu.error("options '-Mn' are incompatible")
    exit 2
  }

  let global_mode = if numeric_requested { "n" } else if human_numeric_requested { "h" } else if general_numeric_requested { "g" } else if month_requested { "M" } else { "" }
  if global_mode != "" and (opts.dictionary or opts.ignore_nonprinting) {
    gnu.error(f"options '{ordering_conflict(opts.dictionary, global_mode)}' are incompatible")
    exit 2
  }

  let has_key = ! opts.key.is_empty()
  let output_path = opts.output.get(0) ?? ""
  let has_output = ! opts.output.is_empty()
  for candidate in opts.output[1..] {
    guard candidate == output_path else {
      gnu.error("multiple output files specified")
      exit 2
    }
  }
  let output = if has_output { fp"{output_path}" } else { p"" }
  let check_mode = if opts.check != "" { opts.check } else if opts.short_check { "diagnose-first" } else { "" }
  let check_enabled = check_mode != "" or opts.silent_check
  let silent_check = opts.silent_check or check_mode in ["silent", "quiet", "silen", "quie", "s", "q"]
  let paths = opts.paths
  # Every occurrence is checked in order, as GNU does while parsing; a separator
  # given again must name the same character as the first one.
  for index in range(opts.delimiter.len()) {
    let raw = opts.delimiter[index]
    if raw == "" {
      gnu.error("empty tab")
      exit 2
    }
    # GNU measures the separator in bytes, so a multibyte character is rejected too.
    let value = separator_value(raw)
    if value.byte_len() != 1 {
      gnu.error(f"multi-character tab {gnu.quote_value(raw)}")
      exit 2
    }
    if value != separator_value(opts.delimiter[0]) {
      gnu.error("incompatible tabs")
      exit 2
    }
  }

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

  var inputs: List[SortInput] = []
  if let list = opts.files0_from {
    guard paths.is_empty() else {
      gnu.error(f"extra operand {gnu.quote(paths[0])}")
      eprint "file operands cannot be combined with --files0-from"
      gnu.try_help()
      exit 2
    }

    let data = if list == "-" {
      io.stdin_bytes()?
    } else {
      match fp"{list}".read_bytes() {
        Ok(data) => data
        Err(failure) => {
          if gnu.errno(failure) == 21 {
            gnu.error(f"cannot read file names from {gnu.quote(list)}")
          } else {
            gnu.error(f"open failed: {gnu.quote_maybe(list)}: {gnu.strerror(failure)}")
          }
          exit 2
        }
      }
    }
    let names = files0_names(data)
    if names.is_empty() {
      gnu.error(f"no input from {gnu.quote(list)}")
      exit 2
    }
    for index in range(names.len()) {
      let name = names[index]
      if name.is_empty() {
        gnu.error(f"{gnu.quote_maybe(list)}:{index + 1}: invalid zero-length file name")
        exit 2
      }
      if list == "-" and name == b"-" {
        gnu.error("when reading file names from standard input, no file name of '-' allowed")
        exit 2
      }
      let input_path = name as Path
      inputs += [{name: name, path: input_path, stdin: name == b"-", compressed: null}]
    }
  } else if paths.is_empty() {
    inputs = [{name: b"-", path: p"-", stdin: true, compressed: null}]
  } else {
    for path_arg in paths {
      inputs += [{name: bytes.from_text(path_arg), path: fp"{path_arg}", stdin: path_arg == "-", compressed: null}]
    }
  }

  var inputs_are_regular = true
  for input in inputs {
    if input.stdin {
      inputs_are_regular = false
    } else {
      match fs.stat(input.path, follow_symlinks: true) {
        Ok(metadata) => if metadata.mode / 4096 % 16 != 8 { inputs_are_regular = false }
        Err(failure) => {
          gnu.error(f"cannot read: {gnu.quote_bytes(input.name, always: false)}: {gnu.strerror(failure)}")
          exit 2
        }
      }
    }
  }

  if opts.merge and ! check_enabled and ! opts.debug and ! has_output and inputs_are_regular {
    let batch_limit = sort_merge_batch_limit(opts)
    if inputs.len() > batch_limit {
      let scratch = create_sort_temp_root(opts.temp_directory)
      defer scratch.close()
      merge_sort_batched(inputs, scratch, opts, has_key, batch_limit, null, opts.unique, startable_compress_program(opts.compress_program))
      return
    }
  }

  if let size = buffer_size {
    if ! opts.merge and ! check_enabled and ! opts.debug and ! random_requested {
      buffered_sort(inputs, opts, size, has_key, output, has_output)
      return
    }
  }

  if can_stream_sort_merge(inputs, opts, check_enabled, inputs_are_regular) {
    stream_sort_merge(inputs, opts, has_key)
    return
  }
  let source_records = read_sort_records(inputs, opts.zero_terminated)
  if has_general_numeric_keys(opts) {
    let numeric_source_lines: List[SortRecord] = collect {
      for records in source_records { yield @records }
    }
    opts = prepare_general_numeric_keys(numeric_source_lines, opts)
  }
  let plan = sort_record_plan(opts, has_key)
  var input_lines: List[SortRecord] = []
  if opts.merge {
    input_lines = merge_sort_records(source_records, opts, has_key)
  } else {
    for records in source_records { input_lines += records }
  }
  if check_enabled {
    if input_lines.len() > 1 {
      for index in range(1, input_lines.len()) {
        let previous = input_lines[index - 1]
        let current = input_lines[index]
        let unordered = sort_record_order(previous, opts, plan) > sort_record_order(current, opts, plan)
        let duplicate = opts.unique and same_sort_record_key(previous, current, opts, plan)

        if unordered or duplicate {
          if ! silent_check {
            let name = inputs[0].name
            let record_ending = if opts.zero_terminated { b"\0" } else { b"\n" }
            io.write_stderr_bytes(bytes.concat([
              bytes.from_text(f"{gnu.prog()}: {gnu.quote_bytes(name, always: false)}:{index + 1}: disorder: "),
              current.bytes,
              record_ending,
            ]))
          }
          exit 1
        }
      }
    }
    return
  }

  let sorted = if random_requested {
    random_sort_records(input_lines, opts, has_key, random_salt(opts.random_source))
  } else {
    sort_records(input_lines, opts, plan)
  }
  let lines = unique_sorted_records(sorted, opts, plan)

  let line_ending = if opts.zero_terminated { b"\0" } else { b"\n" }
  let output_data = if opts.debug {
    debug_sort_records(lines, opts, has_key)
  } else {
    sort_record_bytes(lines, line_ending)
  }

  if has_output {
    if let Err(failure) = output.write(output_data) {
      let action = if let Ok(_) = fs.stat(output, follow_symlinks: true) { "write failed" } else { "open failed" }
      gnu.error(f"{action}: {gnu.quote_maybe(output_path)}: {gnu.strerror(failure)}")
      exit 2
    }
  } else {
    finish_sort_stdout_bytes(output_data)
  }
}
