##! GNU grep: option parsing, file traversal, record scanning, and output.
##!
##! The applets `grep`, `egrep` and `fgrep` share this module. Behavior and
##! diagnostics follow GNU grep: options are permuted like getopt_long
##! (abbreviated long options included), `-NUM` sets the context, binary
##! files are reported once instead of printed, and exit status 2 means an
##! error occurred unless `-q` found a match first.

use gnu
use gnu_regex
use search

const USAGE_HINT = "Usage: grep [OPTION]... PATTERNS [FILE]...\nTry 'grep --help' for more information.\n"
const HELP = """Usage: grep [OPTION]... PATTERNS [FILE]...
Search for PATTERNS in each FILE.
Example: grep -i 'hello world' menu.h main.c
PATTERNS can contain multiple patterns separated by newlines.

Pattern selection and interpretation:
  -E, --extended-regexp     PATTERNS are extended regular expressions
  -F, --fixed-strings       PATTERNS are strings
  -G, --basic-regexp        PATTERNS are basic regular expressions
  -P, --perl-regexp         PATTERNS are Perl regular expressions
  -e, --regexp=PATTERNS     use PATTERNS for matching
  -f, --file=FILE           take PATTERNS from FILE
  -i, --ignore-case         ignore case distinctions in patterns and data
      --no-ignore-case      do not ignore case distinctions (default)
  -w, --word-regexp         match only whole words
  -x, --line-regexp         match only whole lines
  -z, --null-data           a data line ends in 0 byte, not newline

Miscellaneous:
  -s, --no-messages         suppress error messages
  -v, --invert-match        select non-matching lines
  -V, --version             display version information and exit
      --help                display this help text and exit

Output control:
  -m, --max-count=NUM       stop after NUM selected lines
  -b, --byte-offset         print the byte offset with output lines
  -n, --line-number         print line number with output lines
      --line-buffered       flush output on every line
  -H, --with-filename       print file name with output lines
  -h, --no-filename         suppress the file name prefix on output
      --label=LABEL         use LABEL as the standard input file name prefix
  -o, --only-matching       show only nonempty parts of lines that match
  -q, --quiet, --silent     suppress all normal output
      --binary-files=TYPE   assume that binary files are TYPE;
                            TYPE is 'binary', 'text', or 'without-match'
  -a, --text                equivalent to --binary-files=text
  -I                        equivalent to --binary-files=without-match
  -d, --directories=ACTION  how to handle directories;
                            ACTION is 'read', 'recurse', or 'skip'
  -D, --devices=ACTION      how to handle devices, FIFOs and sockets;
                            ACTION is 'read' or 'skip'
  -r, --recursive           like --directories=recurse
  -R, --dereference-recursive  likewise, but follow all symlinks
      --include=GLOB        search only files that match GLOB (a file pattern)
      --exclude=GLOB        skip files that match GLOB
      --exclude-from=FILE   skip files that match any file pattern from FILE
      --exclude-dir=GLOB    skip directories that match GLOB
  -L, --files-without-match  print only names of FILEs with no selected lines
  -l, --files-with-matches  print only names of FILEs with selected lines
  -c, --count               print only a count of selected lines per FILE
  -T, --initial-tab         make tabs line up (if needed)
  -Z, --null                print 0 byte after FILE name

Context control:
  -B, --before-context=NUM  print NUM lines of leading context
  -A, --after-context=NUM   print NUM lines of trailing context
  -C, --context=NUM         print NUM lines of output context
  -NUM                      same as --context=NUM
      --group-separator=SEP  print SEP on line between matches with context
      --no-group-separator  do not print separator for matches with context
      --color[=WHEN],
      --colour[=WHEN]       use markers to highlight the matching strings;
                            WHEN is 'always', 'never', or 'auto'
  -U, --binary              do not strip CR characters at EOL (MSDOS/Windows)

When FILE is '-', read standard input.  If no FILE is given, read standard
input, but with -r, recursively search the working directory instead.  With
fewer than two FILEs, assume -h.  Exit status is 0 if any line is selected,
1 otherwise; if any error occurs and -q is not given, the exit status is 2.

Report bugs to: bug-grep@gnu.org
GNU grep home page: <https://www.gnu.org/software/grep/>
General help using GNU software: <https://www.gnu.org/gethelp/>
"""

# The scan buffer matches GNU grep's initial buffer, so binary detection
# happens at the same granularity.
const CHUNK = 98304
const INTMAX = 9223372036854775807

# `arg` is 0 for none, 1 required, 2 optional. Entries with the same key are
# one option under two spellings, which never makes an abbreviation
# ambiguous. The order is GNU grep's, which fixes the order of the
# possibilities listed in an ambiguity diagnostic.
type LongOption = {name: Str, arg: Int, key: Str}
type Exclusion = {pattern: Str, include: Bool}
type Colors = {on: Bool, reversed: Bool, erase: Bool, selected_match: Str, context_match: Str, selected_line: Str, context_line: Str, filename: Str, line_number: Str, byte_offset: Str, separator: Str}

type Options = {
  matcher: Str,
  keys: List[Bytes],
  keys_given: Bool,
  ignore_case: Bool,
  invert: Bool,
  word: Bool,
  whole: Bool,
  null_data: Bool,
  count: Bool,
  list: Int,
  quiet: Bool,
  suppress: Bool,
  only: Bool,
  filename_option: Int,
  number: Bool,
  byte: Bool,
  tabs: Bool,
  null_name: Bool,
  binary_files: Str,
  before: Int,
  after: Int,
  context: Int,
  max_count: Int,
  directories: Str,
  devices: Str,
  dereference: Bool,
  label: Bytes,
  color: Int,
  group_separator: Bytes?,
  line_buffered: Bool,
  excludes: List[Exclusion],
  exclude_dirs: List[Str],
  show_help: Bool,
  show_version: Bool,
}

type Context = {o: Options, m: gnu_regex.Matcher, c: Colors, utf8: Bool, prefilter: Str?, quiet_output: Bool, report_binary: Bool, done_on_match: Bool, eol: Int, ctx_given: Bool, before: Int, after: Int}
type Run = {used: Bool, selected: Bool, failed: Bool}
type Scan = {count: Int, used: Bool, ok: Bool}
type Rendered = {data: Bytes, ok: Bool}
type Decoded = gnu_regex.Decoded

pure long_options() -> List[LongOption] {
  [
    {name: "basic-regexp", arg: 0, key: "G"}, {name: "extended-regexp", arg: 0, key: "E"},
    {name: "fixed-regexp", arg: 0, key: "F"}, {name: "fixed-strings", arg: 0, key: "F"},
    {name: "perl-regexp", arg: 0, key: "P"}, {name: "after-context", arg: 1, key: "A"},
    {name: "before-context", arg: 1, key: "B"}, {name: "binary-files", arg: 1, key: "binary-files"},
    {name: "byte-offset", arg: 0, key: "b"}, {name: "context", arg: 1, key: "C"},
    {name: "color", arg: 2, key: "color"}, {name: "colour", arg: 2, key: "color"},
    {name: "count", arg: 0, key: "c"}, {name: "dereference-recursive", arg: 0, key: "R"},
    {name: "devices", arg: 1, key: "D"}, {name: "directories", arg: 1, key: "d"},
    {name: "exclude", arg: 1, key: "exclude"}, {name: "exclude-from", arg: 1, key: "exclude-from"},
    {name: "exclude-dir", arg: 1, key: "exclude-dir"}, {name: "file", arg: 1, key: "f"},
    {name: "files-with-matches", arg: 0, key: "l"}, {name: "files-without-match", arg: 0, key: "L"},
    {name: "group-separator", arg: 1, key: "group-separator"}, {name: "help", arg: 0, key: "help"},
    {name: "include", arg: 1, key: "include"}, {name: "ignore-case", arg: 0, key: "i"},
    {name: "no-ignore-case", arg: 0, key: "no-ignore-case"}, {name: "initial-tab", arg: 0, key: "T"},
    {name: "label", arg: 1, key: "label"}, {name: "line-buffered", arg: 0, key: "line-buffered"},
    {name: "line-number", arg: 0, key: "n"}, {name: "line-regexp", arg: 0, key: "x"},
    {name: "max-count", arg: 1, key: "m"}, {name: "no-filename", arg: 0, key: "h"},
    {name: "no-group-separator", arg: 0, key: "no-group-separator"}, {name: "no-messages", arg: 0, key: "s"},
    {name: "null", arg: 0, key: "Z"}, {name: "null-data", arg: 0, key: "z"},
    {name: "only-matching", arg: 0, key: "o"}, {name: "quiet", arg: 0, key: "q"},
    {name: "recursive", arg: 0, key: "r"}, {name: "regexp", arg: 1, key: "e"},
    {name: "invert-match", arg: 0, key: "v"}, {name: "silent", arg: 0, key: "q"},
    {name: "text", arg: 0, key: "a"}, {name: "binary", arg: 0, key: "U"},
    {name: "version", arg: 0, key: "V"}, {name: "with-filename", arg: 0, key: "H"},
    {name: "word-regexp", arg: 0, key: "w"},
  ]
}

# Short options and whether they take an argument.
pure short_takes_argument(ch: Str) -> Int {
  if ch in ["A", "B", "C", "D", "X", "d", "e", "f", "m"] { return 1 }
  if ch in ["E", "F", "G", "H", "I", "P", "T", "U", "V", "a", "b", "c", "h", "i", "L", "l", "n", "o", "q", "R", "r", "s", "u", "v", "w", "x", "y", "Z", "z"] { return 0 }
  -1
}

# Output is buffered by the runtime and a failed write only shows when the
# buffer is flushed, so scans flush as they go: a closed pipe ends the
# search early, as SIGPIPE ends GNU grep.
proc flush_stdout() {
  let flushed = io.flush_stdout()
  if let Err(failure) = flushed {
    if gnu.errno(failure) == 32 { exit 141 }
    eprint f"grep: write error: {gnu.strerror(failure)}"
    exit 2
  }
}

# Every diagnostic is prefixed with `grep:` whatever name the applet was
# invoked by, as the shell wrappers egrep and fgrep exec grep.
proc complain(message: Str) {
  flush_stdout()
  eprint f"grep: {message}"
}

proc die(message: Str) {
  complain(message)
  exit 2
}

proc usage_error() {
  flush_stdout()
  eprint USAGE_HINT.byte_slice(0, USAGE_HINT.byte_len() - 1)
  exit 2
}

proc emit(data: Bytes) {
  let written = io.write_stdout_bytes(data)
  if let Err(failure) = written {
    if gnu.errno(failure) == 32 { exit 141 }
    complain(f"write error: {gnu.strerror(failure)}")
    exit 2
  }
}

proc text_of(raw: Bytes) [env] -> Str {
  match raw.utf8() {
    Ok(text) => text
    Err(_) => gnu.quote_value_bytes(raw)
  }
}

# `xstrtoimax` with base 10 and no suffixes: leading blanks, an optional
# sign, then digits to the end. Overflow saturates and is accepted.
pure parse_intmax(raw: Bytes) -> Int? {
  var at = 0
  let n = raw.len()
  while at < n and (raw.byte_at(at) ?? 0) in [32, 9, 10, 11, 12, 13] { at += 1 }
  var negative = false
  if at < n and raw.byte_at(at) == 45 { negative = true; at += 1 } else if at < n and raw.byte_at(at) == 43 { at += 1 }
  if at >= n { return null }
  var value = 0
  var saturated = false
  while at < n {
    let digit = (raw.byte_at(at) ?? 0) - 48
    if digit < 0 or digit > 9 { return null }
    if value > (INTMAX - digit) / 10 { saturated = true } else if ! saturated { value = value * 10 + digit }
    at += 1
  }
  if saturated { value = INTMAX }
  if negative { -value } else { value }
}

proc detect_utf8() [env] -> Bool {
  var value = ""
  for name in ["LC_ALL", "LC_CTYPE", "LANG"] {
    let found = env.get_or(name, "") ?? ""
    if found != "" { value = found; break }
  }
  let lowered = value.lower()
  lowered.find("utf-8") != null or lowered.find("utf8") != null
}

pure digits_of(value: Int) -> Int {
  var count = 1
  var rest = value
  while rest >= 10 { rest = rest / 10; count += 1 }
  count
}

pure padded(value: Int, width: Int) -> Str {
  var text = f"{value}"
  while text.byte_len() < width { text = " " + text }
  text
}

# Apply one parsed option. Errors are fatal, as in GNU grep, so they surface
# in argument order.
proc apply_option(opts: Options, key: Str, value: Bytes?, digits_context: Int?) [fs, io, process, error] -> Options {
  var o = opts
  let text = text_of(value ?? b"")
  match key {
    "E" => o.matcher = pick_matcher(o.matcher, "E")
    "F" => o.matcher = pick_matcher(o.matcher, "F")
    "G" => o.matcher = pick_matcher(o.matcher, "G")
    "P" => o.matcher = pick_matcher(o.matcher, "P")
    "X" => {
      let name = text
      let mapped = if name == "grep" { "G" } else if name == "egrep" { "E" } else if name == "fgrep" { "F" } else if name == "perl" { "P" } else if name in ["awk", "gawk", "posixawk"] { "A" } else { "" }
      if mapped == "" { die(f"invalid matcher {name}") }
      o.matcher = pick_matcher(o.matcher, mapped)
    }
    "e" => {
      o.keys += [bytes.concat([value ?? b"", b"\n"])]
      o.keys_given = true
    }
    "f" => {
      let contents = read_pattern_file(value ?? b"")
      o.keys += [if contents.is_empty() or contents.byte_at(contents.len() - 1) == 10 { contents } else { bytes.concat([contents, b"\n"]) }]
      o.keys_given = true
    }
    "i" | "y" => o.ignore_case = true
    "no-ignore-case" => o.ignore_case = false
    "v" => o.invert = true
    "w" => o.word = true
    "x" => o.whole = true
    "z" => o.null_data = true
    "c" => o.count = true
    "l" => o.list = 1
    "L" => o.list = -1
    "q" => o.quiet = true
    "s" => o.suppress = true
    "o" => o.only = true
    "H" => o.filename_option = 1
    "h" => o.filename_option = -1
    "n" => o.number = true
    "b" => o.byte = true
    "T" => o.tabs = true
    "Z" => o.null_name = true
    "a" => o.binary_files = "text"
    "I" => o.binary_files = "without-match"
    "U" | "line-buffered" => {
      if key == "line-buffered" { o.line_buffered = true }
    }
    "binary-files" => {
      if text not in ["binary", "text", "without-match"] { die("unknown binary-files type") }
      o.binary_files = text
    }
    "r" => o.directories = "recurse"
    "R" => {
      o.directories = "recurse"
      o.dereference = true
    }
    "d" => {
      if text not in ["read", "recurse", "skip"] {
        flush_stdout()
        eprint f"grep: invalid argument '{text}' for '--directories'"
        eprint "Valid arguments are:\n  - 'read'\n  - 'recurse'\n  - 'skip'"
        eprint USAGE_HINT.byte_slice(0, USAGE_HINT.byte_len() - 1)
        exit 1
      }
      o.directories = text
      if text == "recurse" { o.dereference = false }
    }
    "D" => {
      if text not in ["read", "skip"] { die("unknown devices method") }
      o.devices = text
    }
    "m" => {
      let number = parse_intmax(value ?? b"")
      if number == null { die("invalid max count") }
      o.max_count = if (number ?? 0) < 0 { INTMAX } else { number ?? 0 }
    }
    "A" | "B" | "C" => {
      let number = parse_intmax(value ?? b"") ?? -1
      if number < 0 { die(f"{text}: invalid context length argument") }
      if key == "A" { o.after = number } else if key == "B" { o.before = number } else { o.context = number }
    }
    "context-digits" => o.context = digits_context ?? 0
    "color" => {
      if value == null { o.color = 2 } else {
        let lowered = text.lower()
        if lowered in ["always", "yes", "force"] { o.color = 1 } else if lowered in ["never", "no", "none"] { o.color = 0 } else if lowered in ["auto", "tty", "if-tty"] { o.color = 2 } else { o.show_help = true }
      }
    }
    "label" => o.label = value ?? b""
    "group-separator" => o.group_separator = value
    "no-group-separator" => o.group_separator = null
    "include" => o.excludes += [{pattern: text, include: true}]
    "exclude" => o.excludes += [{pattern: text, include: false}]
    "exclude-dir" => {
      var pattern = text
      while pattern.byte_len() > 1 and pattern.ends_with("/") { pattern = pattern.byte_slice(0, pattern.byte_len() - 1) }
      o.exclude_dirs += [pattern]
    }
    "exclude-from" => {
      let contents = read_pattern_file(value ?? b"")
      for record in search.records(contents, 10) {
        var line = record
        while line.len() > 0 and (line.byte_at(line.len() - 1) ?? 0) in [32, 9, 13, 11, 12] { line = line[0..line.len() - 1] }
        if line.len() > 0 { o.excludes += [{pattern: text_of(line), include: false}] }
      }
    }
    "help" => o.show_help = true
    "V" => o.show_version = true
    _ => usage_error()
  }
  o
}

proc pick_matcher(current: Str, wanted: Str) -> Str {
  if current != "" and current != wanted { die("conflicting matchers specified") }
  wanted
}

proc read_pattern_file(name: Bytes) [fs, io, error] -> Bytes {
  if name == b"-" {
    let read = io.stdin_bytes()
    match read {
      Ok(data) => { return data }
      Err(failure) => { die(f"-: {gnu.strerror(failure)}") }
    }
  }
  let parsed = Path.parse_bytes(name)
  match parsed {
    Ok(target) => {
      let read = target.read_bytes()
      match read {
        Ok(data) => { return data }
        Err(failure) => { die(f"{text_of(name)}: {gnu.strerror(failure)}") }
      }
    }
    Err(failure) => { die(f"{text_of(name)}: {failure.message}") }
  }
  b""
}

# GREP_COLORS: `name=value` capabilities separated by colons. A value may
# hold only digits and semicolons; anything malformed ends parsing, keeping
# what was read so far.
pure apply_capability(base: Colors, name: Str, value: Str?) -> Colors {
  var c = base
  match name {
    "mt" => {
      if value != null { c.selected_match = value ?? "" }
      c.context_match = c.selected_match
    }
    "ms" => { if value != null { c.selected_match = value ?? "" } }
    "mc" => { if value != null { c.context_match = value ?? "" } }
    "sl" => { if value != null { c.selected_line = value ?? "" } }
    "cx" => { if value != null { c.context_line = value ?? "" } }
    "fn" => { if value != null { c.filename = value ?? "" } }
    "ln" => { if value != null { c.line_number = value ?? "" } }
    "bn" => { if value != null { c.byte_offset = value ?? "" } }
    "se" => { if value != null { c.separator = value ?? "" } }
    "rv" => c.reversed = true
    "ne" => c.erase = false
    _ => {}
  }
  c
}

pure parse_colors(spec: Str, base: Colors) -> Colors {
  var c = base
  var name = ""
  var value: Str? = null
  let n = spec.byte_len()
  for at in range(n + 1) {
    let ch = if at < n { spec.byte_slice(at, 1) } else { "" }
    if ch == ":" or ch == "" {
      c = apply_capability(c, name, value)
      name = ""
      value = null
    } else if ch == "=" {
      if name == "" or value != null { return c }
      value = ""
    } else if value == null {
      name += ch
    } else if ch == ";" or (ch >= "0" and ch <= "9") {
      value = (value ?? "") + ch
    } else { return c }
  }
  c
}

pure sgr_start(c: Colors, color: Str) -> Bytes {
  if color == "" { return b"" }
  bytes.from_text(if c.erase { f"\u{1b}[{color}m\u{1b}[K" } else { f"\u{1b}[{color}m" })
}

pure sgr_end(c: Colors, color: Str) -> Bytes {
  if color == "" { return b"" }
  bytes.from_text(if c.erase { "\u{1b}[m\u{1b}[K" } else { "\u{1b}[m" })
}

pure paint(ctx: Context, color: Str, data: Bytes) -> Bytes {
  if ! ctx.c.on or color == "" { return data }
  bytes.concat([sgr_start(ctx.c, color), data, sgr_end(ctx.c, color)])
}

pure separator_byte(ctx: Context, sep: Int) -> Bytes {
  paint(ctx, ctx.c.separator, bytes.from_ints([sep]) ?? b"")
}

# The prefix GNU grep prints before a line or match: file name, line
# number, byte offset and the optional alignment tab.
pure line_head(ctx: Context, show_name: Bool, label: Bytes, lineno: Int, offset: Int, sep: Int, width: Int, length: Int) -> Bytes {
  var parts: List[Bytes] = []
  if show_name {
    parts += [paint(ctx, ctx.c.filename, label)]
    parts += [if ctx.o.null_name { b"\0" } else { separator_byte(ctx, sep) }]
  }
  if ctx.o.number {
    parts += [paint(ctx, ctx.c.line_number, bytes.from_text(padded(lineno, width))), separator_byte(ctx, sep)]
  }
  if ctx.o.byte {
    parts += [paint(ctx, ctx.c.byte_offset, bytes.from_text(padded(offset, width))), separator_byte(ctx, sep)]
  }
  if ctx.o.tabs and (show_name or ctx.o.number or ctx.o.byte) and length > 0 { parts += [b"\t"] }
  bytes.concat(parts)
}

# A record that is not valid UTF-8 cannot be printed in a UTF-8 locale.
pure invalid_text(ctx: Context, data: Bytes) -> Bool {
  if ! ctx.utf8 { return false }
  match data.utf8() {
    Ok(_) => false
    Err(_) => true
  }
}

# Render one record the way GNU grep's prline does. `sep` is 58 for a
# selected line and 45 for context. A record whose bytes are not valid in a
# UTF-8 locale is withheld (`ok` false) so the caller can report that the
# binary file matches.
pure render_line(ctx: Context, line: Bytes, dec: Decoded, sep: Int, show_name: Bool, label: Bytes, lineno: Int, offset: Int, width: Int) -> Rendered {
  let o = ctx.o
  let selected = sep == 58
  let matching = selected != o.invert
  let text_mode = o.binary_files == "text"
  let eol = bytes.from_ints([ctx.eol]) ?? b"\n"
  if ! o.only and ! text_mode and invalid_text(ctx, line) { return {data: b"", ok: false} }
  var line_color = ""
  var match_color = ""
  if ctx.c.on {
    line_color = if selected != (o.invert and ctx.c.reversed) { ctx.c.selected_line } else { ctx.c.context_line }
    match_color = if selected { ctx.c.selected_match } else { ctx.c.context_match }
  }
  var parts: List[Bytes] = []
  if ! o.only { parts += [line_head(ctx, show_name, label, lineno, offset, sep, width_for(ctx, width), line.len())] }
  var cur = 0
  let total = line.len()
  if (o.only and matching) or (ctx.c.on and (line_color != "" or match_color != "")) {
    if matching and (o.only or match_color != "") {
      var from = 0
      while from <= total {
        let found = gnu_regex.find_bytes(ctx.m, line, dec, from, true)
        if found.is_empty() { break }
        let b = found[0]
        let e = found[1]
        if e == b {
          # An empty match shows nothing; resume one character later.
          from = b + 1
          while from < total and ctx.utf8 and (line.byte_at(from) ?? 0) >= 128 and (line.byte_at(from) ?? 0) < 192 { from += 1 }
          continue
        }
        if o.only {
          if ! text_mode and invalid_text(ctx, line[b..e]) { return {data: bytes.concat(parts), ok: false} }
          let head_sep = if o.invert { 45 } else { 58 }
          parts += [line_head(ctx, show_name, label, lineno, offset + b, head_sep, width_for(ctx, width), e - b)]
          parts += [paint(ctx, match_color, line[b..e]), eol]
        } else {
          parts += [sgr_start(ctx.c, line_color), line[cur..b], paint(ctx, match_color, line[b..e])]
        }
        cur = e
        from = e
      }
      if o.only { cur = total + 1 }
    }
    if ! o.only and line_color != "" and cur <= total {
      let rest = line[cur..]
      var tail = rest.len()
      if tail > 0 and rest.byte_at(tail - 1) == 13 { tail -= 1 }
      if tail > 0 {
        parts += [sgr_start(ctx.c, line_color), rest[0..tail], sgr_end(ctx.c, line_color)]
        cur += tail
      }
    }
  }
  if ! o.only { parts += [line[cur..], eol] }
  {data: bytes.concat(parts), ok: true}
}

pure width_for(ctx: Context, width: Int) -> Int {
  if ctx.o.tabs { width } else { 0 }
}

# Positions of record delimiters in a buffer. The common case (newline
# records, no NUL bytes) is one native search; NUL bytes, `-z`, and binary
# zapping take a byte loop.
pure delimiter_positions(data: Bytes, eol: Int, zap: Bool) -> List[Int] {
  if eol == 10 and ! zap {
    match regex.find_bytes("\n", data) {
      Ok(marks) => { return [mark.start for mark in marks] }
      Err(_) => {}
    }
  }
  var out: List[Int] = []
  for at in range(data.len()) {
    let byte = data.byte_at(at) ?? 0
    if byte == eol or (zap and byte == 0) { out += [at] }
  }
  out
}

pure has_nul(data: Bytes) -> Bool {
  match regex.find_bytes("\n", data) {
    Ok(_) => false
    Err(_) => true
  }
}

# Scan one input. `mode` is "stdin", "seek" (regular file, bounded reads) or
# "whole" (device or pipe read in one piece). Returns the number of selected
# lines; output and diagnostics are produced as GNU grep does.
proc scan(ctx: Context, target: Path?, mode: Str, label: Bytes, show_name: Bool, used_in: Bool, width: Int, size: Int) [fs, io, error] -> Scan {
  let o = ctx.o
  var used = used_in
  var quiet = ctx.quiet_output
  var done = ctx.done_on_match
  var count = 0
  var outleft = o.max_count
  var pending = 0
  var ring_lines: List[Bytes] = []
  var ring_numbers: List[Int] = []
  var ring_offsets: List[Int] = []
  var lastout = -1
  var lineno = 0
  var consumed = 0
  var leftover = b""
  var zap = false
  var first_null = -1
  var encoding_hit = false
  var stopped = false
  var failed = false
  let text_mode = o.binary_files == "text"
  let detect_nuls = ctx.eol == 10 and ! text_mode
  var offset = 0
  var flushed_at = 0
  # With a selection limit, GNU grep's list modes consider the whole file
  # before the first selected line, so a NUL anywhere makes it binary.
  if o.list != 0 and o.max_count != INTMAX and detect_nuls and mode == "seek" {
    var probe = 0
    while probe < size and ! zap {
      let want = if size - probe < CHUNK { size - probe } else { CHUNK }
      let read = bytes.read_at(target ?? p"", probe, want)
      match read {
        Ok(data) => {
          if data.is_empty() { break }
          if has_nul(data) { zap = true; first_null = 0 }
          probe += data.len()
        }
        Err(_) => { break }
      }
    }
  }
  while ! stopped {
    var chunk = b""
    if mode == "stdin" {
      let read = io.stdin_read(CHUNK)
      match read {
        Ok(data) => { chunk = data }
        Err(failure) => { complain(f"{text_of(label)}: {gnu.strerror(failure)}"); failed = true; stopped = true }
      }
    } else if mode == "whole" {
      if consumed == 0 {
        let read = (target ?? p"").read_bytes()
        match read {
          Ok(data) => { chunk = data }
          Err(failure) => { if ! o.suppress { complain(f"{text_of(label)}: {gnu.strerror(failure)}") }; failed = true; stopped = true }
        }
      }
    } else if consumed < size {
      let want = if size - consumed < CHUNK { size - consumed } else { CHUNK }
      let read = bytes.read_at(target ?? p"", consumed, want)
      match read {
        Ok(data) => { chunk = data }
        Err(failure) => { if ! o.suppress { complain(f"{text_of(label)}: {gnu.strerror(failure)}") }; failed = true; stopped = true }
      }
    }
    if stopped { break }
    consumed += chunk.len()
    let eof = chunk.is_empty()
    if detect_nuls and ! zap and has_nul(chunk) {
      if o.binary_files == "without-match" { return {count: 0, used: used, ok: true} }
      if ! o.count { done = true; quiet = true }
      first_null = count
      zap = true
    }
    let data = if leftover.is_empty() { chunk } else { bytes.concat([leftover, chunk]) }
    let marks = delimiter_positions(data, ctx.eol, zap)
    var begin = 0
    var records: List[Bytes] = []
    for mark in marks {
      records += [data[begin..mark]]
      begin = mark + 1
    }
    leftover = data[begin..]
    if eof and ! leftover.is_empty() { records += [leftover]; leftover = b"" }
    # Plain strings: one native search over the buffer finds which records
    # can match at all, so the rest skip per-record matching.
    var possible: List[Bool] = []
    if ctx.prefilter != null and ! zap and ctx.eol == 10 and ! records.is_empty() {
      match regex.find_bytes(ctx.prefilter ?? "", data, extended: true, ignore_case: o.ignore_case) {
        Ok(found) => {
          var cursor = 0
          var record_begin = 0
          for record in records {
            let record_end = record_begin + record.len()
            var seen = false
            while cursor < found.len() and found[cursor].start <= record_end {
              if found[cursor].start >= record_begin { seen = true }
              cursor += 1
            }
            possible += [seen]
            record_begin = record_end + 1
          }
        }
        Err(_) => {}
      }
    }
    var record_index = -1
    for line in records {
      record_index += 1
      lineno += 1
      let line_offset = offset
      offset += line.len() + 1
      let skip = ! possible.is_empty() and ! possible[record_index]
      let decoded = if skip { {chars: [], offs: [], native: true} } else { gnu_regex.subject(ctx.m, line) }
      let hit = ! skip and ! gnu_regex.find_bytes(ctx.m, line, decoded, 0, false).is_empty()
      let selected = hit != o.invert
      if outleft == 0 {
        if pending > 0 and ! quiet {
          let r = render_line(ctx, line, decoded, 45, show_name, label, lineno, line_offset, width)
          if r.ok { emit(r.data); lastout = lineno } else { encoding_hit = true }
          pending -= 1
          if pending == 0 { stopped = true }
        } else { stopped = true }
      } else if selected {
        count += 1
        outleft -= 1
        if ! quiet {
          var first = lineno
          if ! ring_lines.is_empty() { first = ring_numbers[0] }
          if ctx.ctx_given and used and first != lastout + 1 and o.group_separator != null {
            emit(bytes.concat([paint(ctx, ctx.c.separator, o.group_separator ?? b""), b"\n"]))
          }
          for index in range(ring_lines.len()) {
            let prior = gnu_regex.subject(ctx.m, ring_lines[index])
            let r = render_line(ctx, ring_lines[index], prior, 45, show_name, label, ring_numbers[index], ring_offsets[index], width)
            if r.ok { emit(r.data); lastout = ring_numbers[index] } else { encoding_hit = true }
          }
          ring_lines = []
          ring_numbers = []
          ring_offsets = []
          let r = render_line(ctx, line, decoded, 58, show_name, label, lineno, line_offset, width)
          if r.ok { emit(r.data); lastout = lineno } else { encoding_hit = true }
          if o.line_buffered { flush_stdout() }
          pending = if ctx.after > 0 { ctx.after } else { 0 }
        }
        # GNU grep marks output as started even for a selected line it did
        # not print (a binary file), so the next file's group gets its
        # separator.
        used = true
        if done { stopped = true }
        if outleft == 0 and pending == 0 { stopped = true }
      } else if pending > 0 and ! quiet {
        let r = render_line(ctx, line, decoded, 45, show_name, label, lineno, line_offset, width)
        if r.ok { emit(r.data); lastout = lineno } else { encoding_hit = true }
        pending -= 1
      } else if ctx.before > 0 and ! quiet {
        ring_lines += [line]
        ring_numbers += [lineno]
        ring_offsets += [line_offset]
        if ring_lines.len() > ctx.before {
          ring_lines = ring_lines[1..]
          ring_numbers = ring_numbers[1..]
          ring_offsets = ring_offsets[1..]
        }
      }
      if stopped { break }
      if offset - flushed_at > 65536 {
        flush_stdout()
        flushed_at = offset
      }
    }
    if eof { break }
  }
  if o.binary_files == "binary" and ctx.report_binary and (encoding_hit or (first_null >= 0 and first_null < count)) {
    complain(f"{text_of(label)}: binary file matches")
  }
  {count: count, used: used, ok: ! failed}
}

# gnulib's exclude list: the last pattern that matches decides; when none
# match, the first pattern's polarity decides (an include list excludes
# everything else). Command-line names may match any trailing path
# component run; names found by recursion are matched whole.
pure glob_matches(pattern: Str, name: Bytes, anchored: Bool) -> Bool {
  if search.glob(pattern, name) { return true }
  if anchored { return false }
  for at in range(name.len() - 1) {
    if name.byte_at(at) == 47 and name.byte_at(at + 1) != 47 {
      if search.glob(pattern, name[at + 1..]) { return true }
    }
  }
  false
}

pure excluded_file(rules: List[Exclusion], name: Bytes, anchored: Bool) -> Bool {
  if rules.is_empty() { return false }
  var index = rules.len() - 1
  while index >= 0 {
    if glob_matches(rules[index].pattern, name, anchored) { return ! rules[index].include }
    index -= 1
  }
  rules[0].include
}

pure excluded_dir(patterns: List[Str], name: Bytes, anchored: Bool) -> Bool {
  for pattern in patterns { if glob_matches(pattern, name, anchored) { return true } }
  false
}

pure join_path(display: Bytes, name: Bytes) -> Bytes {
  if display.is_empty() { return name }
  if display.byte_at(display.len() - 1) == 47 { return bytes.concat([display, name]) }
  bytes.concat([display, b"/", name])
}

# How fts names a root: two or more trailing slashes shrink to one, and a
# single trailing slash stays (so `d/` is not matched by the pattern `d`).
pure fts_root_name(raw: Bytes) -> Bytes {
  var end = raw.len()
  if end > 2 and raw.byte_at(end - 1) == 47 {
    while end > 1 and raw.byte_at(end - 2) == 47 { end -= 1 }
  }
  raw[0..end]
}

proc open_error(ctx: Context, label: Bytes, message: Str) -> Unit {
  if ! ctx.o.suppress { complain(f"{text_of(label)}: {message}") }
}

# Print `-c`, `-l` and `-L` output for one scanned input and fold its
# result into the run status.
proc finish_input(ctx: Context, progress: Run, scanned: Scan, label: Bytes, show_name: Bool) -> Run {
  let o = ctx.o
  var out = progress
  out.used = scanned.used
  if ! scanned.ok { out.failed = true }
  if o.count and o.list == 0 and ! o.quiet {
    var parts: List[Bytes] = []
    if show_name {
      parts += [paint(ctx, ctx.c.filename, label)]
      parts += [if o.null_name { b"\0" } else { separator_byte(ctx, 58) }]
    }
    parts += [bytes.from_text(f"{scanned.count}\n")]
    emit(bytes.concat(parts))
    if o.line_buffered { flush_stdout() }
  }
  var listed = false
  if o.list == 1 and ! o.quiet { listed = scanned.count > 0 } else if o.list == -1 and ! o.quiet { listed = scanned.count == 0 }
  if listed { emit(bytes.concat([paint(ctx, ctx.c.filename, label), if o.null_name { b"\0" } else { b"\n" }])) }
  # With -L the status reports whether the file was listed, the inverse of
  # selection, which is what the BusyBox suite requires.
  let matched = if o.list == -1 and ! o.quiet { listed } else { scanned.count > 0 }
  if matched { out.selected = true }
  if o.quiet and scanned.count > 0 {
    flush_stdout()
    exit 0
  }
  out
}

proc search_input(ctx: Context, progress: Run, real: Path?, show_name: Bool, label: Bytes) [fs, io, error] -> Run {
  var mode = "stdin"
  var size = 0
  var width = 0
  var width_source = p"/proc/self/fd/0"
  if real != null { width_source = real ?? p"/proc/self/fd/0" }
  if ctx.o.tabs {
    let meta = fs.stat(width_source, follow_symlinks: true)
    var digits = 19
    match meta {
      Ok(stat) => { if stat.kind == "file" { digits = digits_of(stat.size + (if ctx.o.number { 1 } else { 0 })) } }
      Err(_) => {}
    }
    width = digits
  }
  if real != null {
    let meta = fs.stat(real ?? p"", follow_symlinks: true)
    match meta {
      Ok(stat) => {
        mode = if stat.kind == "file" { "seek" } else { "whole" }
        size = stat.size
        if stat.kind == "file" and stat.size == 0 { mode = "whole" }
      }
      Err(failure) => {
        open_error(ctx, label, gnu.strerror(failure))
        var out = progress
        out.failed = true
        return out
      }
    }
  }
  let scanned = scan(ctx, real, mode, label, show_name, progress.used, width, size)
  finish_input(ctx, progress, scanned, label, show_name)
}

# Visit one operand or directory entry. `show` is 1 or 0 for whether names
# prefix output and -1 when it depends on this being a directory.
proc visit(ctx: Context, progress: Run, display: Bytes, walk_name: Bytes, real: Path, show: Int, command_line: Bool, implicit_root: Bool, ancestors: List[Str]) [fs, io, error] -> Run {
  let o = ctx.o
  let follow = command_line or o.dereference
  let meta = fs.stat(real, follow_symlinks: follow)
  let name = if command_line { walk_name } else { search.basename_bytes(display) }
  var stat_kind = ""
  var identity = ""
  match meta {
    Ok(stat) => {
      stat_kind = stat.kind
      identity = f"{stat.dev}:{stat.ino}"
    }
    Err(failure) => {
      # File filters run on an entry before it is opened, so a filtered-out
      # dangling link is skipped without a diagnostic.
      if ! command_line and excluded_file(o.excludes, name, true) { return progress }
      open_error(ctx, display, gnu.strerror(failure))
      var out = progress
      out.failed = true
      return out
    }
  }
  if stat_kind == "symlink" { return progress }
  if stat_kind == "dir" {
    if o.directories == "skip" { return progress }
    if ! (command_line and implicit_root) and excluded_dir(o.exclude_dirs, name, ! command_line) { return progress }
    let show_names = if show < 0 { true } else { show == 1 }
    if o.directories == "recurse" {
      if identity in ancestors {
        if ! o.suppress { complain(f"{text_of(display)}: warning: recursive directory loop") }
        return progress
      }
      let listing = fs.children(real, ordered: true)
      match listing {
        Ok(entries) => {
          var current = progress
          for entry in entries {
            let leaf = search.basename_bytes(entry.path.bytes())
            let child = search.child(real, leaf)
            match child {
              Ok(entry_path) => {
                let child_name = join_path(if command_line { walk_name } else { display }, leaf)
                current = visit(ctx, current, child_name, child_name, entry_path, if show_names { 1 } else { 0 }, false, false, ancestors + [identity])
              }
              Err(failure) => {
                open_error(ctx, display, failure.message)
                current.failed = true
              }
            }
          }
          return current
        }
        Err(failure) => {
          open_error(ctx, display, gnu.strerror(failure))
          var out = progress
          out.failed = true
          return out
        }
      }
    }
    open_error(ctx, display, "Is a directory")
    return finish_input(ctx, progress, {count: 0, used: progress.used, ok: false}, display, show_names)
  }
  if stat_kind != "file" and (o.devices == "skip" or (! command_line and o.devices != "read")) { return progress }
  if excluded_file(o.excludes, name, ! command_line) { return progress }
  let show_names = show == 1
  search_input(ctx, progress, real, show_names, display)
}

proc color_enabled(mode: Int) [env, io, process, error] -> Bool {
  if mode == 1 { return true }
  if mode == 0 { return false }
  let term = env.get_or("TERM", "") ?? ""
  term != "" and term != "dumb" and unix.isatty(1)
}

## Entry point shared by grep, egrep and fgrep. `flag` is the matcher the
## applet name implies ("E" or "F") and `wrapper` the applet name that warns
## it is obsolescent; both are empty for grep itself.
export proc grep_main(args: List[Bytes], flag: Str, wrapper: Str) [fs, io, process, env, error] -> Unit {
  var o: Options = {
    matcher: "", keys: [], keys_given: false, ignore_case: false, invert: false, word: false, whole: false,
    null_data: false, count: false, list: 0, quiet: false, suppress: false, only: false, filename_option: 0,
    number: false, byte: false, tabs: false, null_name: false, binary_files: "binary", before: -1, after: -1,
    context: -1, max_count: INTMAX, directories: "read", devices: "read-command-line", dereference: false,
    label: b"(standard input)", color: 0, group_separator: b"--", line_buffered: false, excludes: [],
    exclude_dirs: [], show_help: false, show_version: false,
  }
  if flag != "" {
    flush_stdout()
    eprint f"{wrapper}: warning: {wrapper} is obsolescent; using grep -{flag}"
    o.matcher = flag
  }
  let longs = long_options()
  let posixly = (env.get_or("POSIXLY_CORRECT", "\u{0}unset") ?? "") != "\u{0}unset"
  var operands: List[Bytes] = []
  var index = 0
  var options_ended = false
  while index < args.len() {
    let raw = args[index]
    index += 1
    if options_ended or raw.byte_at(0) != 45 or raw.len() == 1 {
      operands += [raw]
      if posixly { options_ended = true }
      continue
    }
    if raw == b"--" { options_ended = true; continue }
    if raw.byte_at(1) == 45 {
      let body = raw[2..]
      var name_bytes = body
      var value: Bytes? = null
      var equals = -1
      for at in range(body.len()) { if body.byte_at(at) == 61 { equals = at; break } }
      if equals >= 0 {
        name_bytes = body[0..equals]
        value = body[equals + 1..]
      }
      let name = text_of(name_bytes)
      var found: LongOption? = null
      var candidates: List[LongOption] = []
      for option in longs {
        if option.name == name { found = option; break }
        if option.name.starts_with(name) { candidates += [option] }
      }
      if found == null {
        var distinct: List[LongOption] = []
        for option in candidates {
          var seen = false
          for known in distinct { if known.key == option.key and known.arg == option.arg { seen = true } }
          if ! seen { distinct += [option] }
        }
        if distinct.len() == 1 { found = distinct[0] } else if distinct.len() > 1 {
          let listed = [f"'--{option.name}'" for option in candidates].join(" ")
          complain(f"option '--{text_of(raw[2..])}' is ambiguous; possibilities: {listed}")
          usage_error()
        } else {
          complain(f"unrecognized option '--{text_of(raw[2..])}'")
          usage_error()
        }
      }
      let option = found ?? longs[0]
      if option.arg == 0 and value != null {
        complain(f"option '--{option.name}' doesn't allow an argument")
        usage_error()
      }
      if option.arg == 1 and value == null {
        if index >= args.len() {
          complain(f"option '--{option.name}' requires an argument")
          usage_error()
        }
        value = args[index]
        index += 1
      }
      o = apply_option(o, option.key, value, null)
      continue
    }
    var position = 1
    var in_digits = false
    var digits = 0
    let text = text_of(raw)
    while position < text.byte_len() {
      let ch = text.byte_slice(position, 1)
      position += 1
      if ch >= "0" and ch <= "9" {
        let digit = (ch.byte_at(0) ?? 48) - 48
        digits = if in_digits { (if digits > (INTMAX - digit) / 10 { INTMAX } else { digits * 10 + digit }) } else { digit }
        in_digits = true
        o = apply_option(o, "context-digits", null, digits)
        continue
      }
      in_digits = false
      let takes = short_takes_argument(ch)
      if takes < 0 {
        complain(f"invalid option -- '{ch}'")
        usage_error()
      }
      if takes == 0 {
        if ch == "u" { usage_error() }
        o = apply_option(o, ch, null, null)
        continue
      }
      var value: Bytes? = null
      if position < text.byte_len() {
        value = bytes.from_text(text.byte_slice(position))
        position = text.byte_len()
      } else if index < args.len() {
        value = args[index]
        index += 1
      } else {
        complain(f"option requires an argument -- '{ch}'")
        usage_error()
      }
      o = apply_option(o, ch, value, null)
    }
  }
  if o.show_version {
    gnu.version("grep")
    return
  }
  if o.show_help {
    emit(bytes.from_text(HELP))
    flush_stdout()
    return
  }
  var keys = b""
  if o.keys_given {
    keys = bytes.concat(o.keys)
  } else {
    if operands.is_empty() { usage_error() }
    keys = bytes.concat([operands[0], b"\n"])
    operands = operands[1..]
  }
  var invert = o.invert
  var whole = o.whole
  var word = o.word
  var patterns: List[Bytes] = []
  if keys.is_empty() {
    invert = ! invert
    whole = false
    word = false
    patterns = [b""]
  } else {
    let stripped = keys[0..keys.len() - 1]
    patterns = []
    var begin = 0
    for at in range(stripped.len()) {
      if stripped.byte_at(at) == 10 {
        patterns += [stripped[begin..at]]
        begin = at + 1
      }
    }
    patterns += [stripped[begin..]]
  }
  let nothing_selected = patterns.len() == 1 and patterns[0].is_empty() and invert and ! whole and ! word
  var list = o.list
  var count = o.count
  var quiet_output = o.count
  var done = false
  if o.quiet { list = 0 }
  if o.quiet or list != 0 { count = false; done = true }
  quiet_output = count or done
  if (o.max_count == 0 or nothing_selected) and list != -1 { exit 1 }
  if o.matcher == "P" { die("Perl matching not supported in a --disable-perl-regexp build") }
  if o.matcher == "A" { die("awk matchers are not supported") }
  let utf8 = detect_utf8()
  var programs: List[gnu_regex.Program] = []
  var warnings: List[Str] = []
  for pattern in patterns {
    let compiled = if o.matcher == "F" { gnu_regex.compile_fixed(pattern, o.ignore_case, utf8) } else { gnu_regex.compile_one(pattern, o.matcher == "E", o.ignore_case, utf8) }
    match compiled {
      Ok(done_compile) => {
        programs += done_compile.programs
        warnings += done_compile.warnings
      }
      Err(failure) => { die(failure.message) }
    }
  }
  for warning in warnings { complain(f"warning: {warning}") }
  var colors: Colors = {on: false, reversed: false, erase: true, selected_match: "01;31", context_match: "01;31", selected_line: "", context_line: "", filename: "35", line_number: "32", byte_offset: "32", separator: "36"}
  let colorize = color_enabled(o.color)
  if colorize {
    colors.on = true
    let legacy = env.get_or("GREP_COLOR", "") ?? ""
    var legacy_ok = legacy != ""
    for at in range(legacy.byte_len()) {
      let ch = legacy.byte_slice(at, 1)
      if ch != ";" and (ch < "0" or ch > "9") { legacy_ok = false }
    }
    if legacy_ok {
      colors.selected_match = legacy
      colors.context_match = legacy
    }
    colors = parse_colors(env.get_or("GREP_COLORS", "") ?? "", colors)
    if legacy_ok and (colors.selected_match == legacy or colors.context_match == legacy) {
      complain(f"warning: GREP_COLOR='{legacy}' is deprecated; use GREP_COLORS='mt={legacy}'")
    }
  }
  var before = o.before
  var after = o.after
  if after < 0 { after = o.context }
  if before < 0 { before = o.context }
  let ctx_given = before >= 0 or after >= 0
  var settled = o
  settled.invert = invert
  settled.whole = whole
  settled.word = word
  settled.list = list
  settled.count = count
  let matcher = gnu_regex.matcher(programs, utf8, word, whole)
  let prefilter = gnu_regex.alternation(programs)
  let ctx: Context = {o: settled, m: matcher, c: colors, utf8: utf8, prefilter: prefilter, quiet_output: quiet_output, report_binary: ! (count or o.quiet or (list != 0 and o.max_count == INTMAX)), done_on_match: done, eol: if o.null_data { 0 } else { 10 }, ctx_given: ctx_given, before: if before > 0 { before } else { 0 }, after: if after > 0 { after } else { 0 }}
  var targets = operands
  var implicit_root = false
  if targets.is_empty() {
    if o.directories == "recurse" {
      targets = [b"."]
      implicit_root = true
    } else { targets = [b"-"] }
  }
  let recursive = o.directories == "recurse"
  var show = -1
  if o.filename_option == 0 and operands.len() <= 1 { show = if recursive { -1 } else { 0 } } else if o.filename_option >= 0 { show = 1 } else { show = 0 }
  var progress: Run = {used: false, selected: false, failed: false}
  for target in targets {
    if target == b"-" and ! implicit_root {
      progress = search_input(ctx, progress, null, show == 1, o.label)
      continue
    }
    let rooted = if implicit_root { b"" } else { fts_root_name(target) }
    let parsed = Path.parse_bytes(if implicit_root { b"." } else { target })
    match parsed {
      Ok(real) => { progress = visit(ctx, progress, if implicit_root { b"" } else { target }, rooted, real, show, true, implicit_root, []) }
      Err(failure) => {
        open_error(ctx, target, failure.message)
        progress.failed = true
      }
    }
  }
  flush_stdout()
  let flushed = io.flush_stdout()
  if let Err(failure) = flushed {
    complain(f"write error: {gnu.strerror(failure)}")
    exit 2
  }
  exit if progress.failed { 2 } else if progress.selected { 0 } else { 1 }
}
