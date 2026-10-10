#!/bin/xsh
use lib.gnu
use lib.search
use lib.gnudiff
use lib.diffrender
use lib.textio_a1 as tio

const HELP = """Usage: diff [OPTION]... FILES
Compare FILES line by line.

Mandatory arguments to long options are mandatory for short options too.
      --normal                  output a normal diff (the default)
  -q, --brief                   report only when files differ
  -s, --report-identical-files  report when two files are the same
  -c, -C NUM, --context[=NUM]   output NUM (default 3) lines of copied context
  -u, -U NUM, --unified[=NUM]   output NUM (default 3) lines of unified context
  -e, --ed                      output an ed script
  -n, --rcs                     output an RCS format diff
  -y, --side-by-side            output in two columns
  -W, --width=NUM               output at most NUM (default 130) print columns
      --left-column             output only the left column of common lines
      --suppress-common-lines   do not output common lines

  -p, --show-c-function         show which C function each change is in
  -F, --show-function-line=RE   show the most recent line matching RE
      --label LABEL             use LABEL instead of file name and timestamp
                                  (can be repeated)

  -t, --expand-tabs             expand tabs to spaces in output
  -T, --initial-tab             make tabs line up by prepending a tab
      --tabsize=NUM             tab stops every NUM (default 8) print columns
      --suppress-blank-empty    suppress space or tab before empty output lines
  -l, --paginate                pass output through 'pr' to paginate it

  -r, --recursive                 recursively compare any subdirectories found
      --no-dereference            don't follow symbolic links
  -N, --new-file                  treat absent files as empty
      --unidirectional-new-file   treat absent first files as empty
      --ignore-file-name-case     ignore case when comparing file names
      --no-ignore-file-name-case  consider case when comparing file names
  -x, --exclude=PAT               exclude files that match PAT
  -X, --exclude-from=FILE         exclude files that match any pattern in FILE
  -S, --starting-file=FILE        start with FILE when comparing directories
      --from-file=FILE1           compare FILE1 to all operands;
                                    FILE1 can be a directory
      --to-file=FILE2             compare all operands to FILE2;
                                    FILE2 can be a directory

  -i, --ignore-case               ignore case differences in file contents
  -E, --ignore-tab-expansion      ignore changes due to tab expansion
  -Z, --ignore-trailing-space     ignore white space at line end
  -b, --ignore-space-change       ignore changes in the amount of white space
  -w, --ignore-all-space          ignore all white space
  -B, --ignore-blank-lines        ignore changes where lines are all blank
  -I, --ignore-matching-lines=RE  ignore changes where all lines match RE

  -a, --text                      treat all files as text
      --strip-trailing-cr         strip trailing carriage return on input

  -D, --ifdef=NAME                output merged file with '#ifdef NAME' diffs
      --GTYPE-group-format=GFMT   format GTYPE input groups with GFMT
      --line-format=LFMT          format all input lines with LFMT
      --LTYPE-line-format=LFMT    format LTYPE input lines with LFMT
    These format options provide fine-grained control over the output
      of diff, generalizing -D/--ifdef.
    LTYPE is 'old', 'new', or 'unchanged'.  GTYPE is LTYPE or 'changed'.
    GFMT (only) may contain:
      %<  lines from FILE1
      %>  lines from FILE2
      %=  lines common to FILE1 and FILE2
      %[-][WIDTH][.[PREC]]{doxX}LETTER  printf-style spec for LETTER
        LETTERs are as follows for new group, lower case for old group:
          F  first line number
          L  last line number
          N  number of lines = L-F+1
          E  F-1
          M  L+1
      %(A=B?T:E)  if A equals B then T else E
    LFMT (only) may contain:
      %L  contents of line
      %l  contents of line, excluding any trailing newline
      %[-][WIDTH][.[PREC]]{doxX}n  printf-style spec for input line number
    Both GFMT and LFMT may contain:
      %%  %
      %c'C'  the single character C
      %c'\\OOO'  the character with octal code OOO
      C    the character C (other characters represent themselves)

  -d, --minimal            try hard to find a smaller set of changes
      --horizon-lines=NUM  keep NUM lines of the common prefix and suffix
      --speed-large-files  assume large files and many scattered small changes
      --color[=WHEN]       color output; WHEN is 'never', 'always', or 'auto';
                             plain --color means --color='auto'
      --palette=PALETTE    the colors to use when --color is active; PALETTE is
                             a colon-separated list of terminfo capabilities

      --help               display this help and exit
  -v, --version            output version information and exit

FILES are 'FILE1 FILE2' or 'DIR1 DIR2' or 'DIR FILE' or 'FILE DIR'.
If --from-file or --to-file is given, there are no restrictions on FILE(s).
If a FILE is '-', read standard input.
Exit status is 0 if inputs are the same, 1 if different, 2 if trouble.

Report bugs to: bug-diffutils@gnu.org
GNU diffutils home page: <https://www.gnu.org/software/diffutils/>
General help using GNU software: <https://www.gnu.org/gethelp/>
"""

# Characters that make GNU diff quote an option word in the `diff -r ... a/f
# b/f` line, and the options that take a value, so the line can echo the
# command-line options as typed.
const SHELL_SPECIAL = " \t\n!\"$&'()*;<=>?[\\^`|"
const VALUE_LETTERS = "xXLIFUCWDS"
const OTHER_LONGS = ["normal", "brief", "report-identical-files", "context", "unified", "ed", "rcs", "side-by-side", "left-column", "suppress-common-lines", "show-c-function", "expand-tabs", "initial-tab", "suppress-blank-empty", "paginate", "recursive", "no-dereference", "new-file", "unidirectional-new-file", "ignore-file-name-case", "no-ignore-file-name-case", "ignore-case", "ignore-tab-expansion", "ignore-trailing-space", "ignore-space-change", "ignore-all-space", "ignore-blank-lines", "text", "strip-trailing-cr", "minimal", "speed-large-files", "color", "help", "version"]
const VALUE_LONGS = ["label", "exclude", "exclude-from", "ignore-matching-lines", "show-function-line", "width", "tabsize", "horizon-lines", "starting-file", "from-file", "to-file", "ifdef", "palette", "old-line-format", "new-line-format", "unchanged-line-format", "line-format", "old-group-format", "new-group-format", "changed-group-format", "unchanged-group-format"]

type Options = {normal: Bool, unified: Bool, unified_short: List[Str], unified_long: List[Str], context_c: Bool, context_short: List[Str], context_long: List[Str], digits: List[Int], ed: Bool, rcs: Bool, side: Bool, ifdef: List[Str], line_format: List[Str], old_line_format: List[Str], new_line_format: List[Str], unchanged_line_format: List[Str], old_group_format: List[Str], new_group_format: List[Str], changed_group_format: List[Str], unchanged_group_format: List[Str], width: Str?, left_column: Bool, suppress_common: Bool, brief: Bool, report_same: Bool, show_c: Bool, show_function: List[Str], label: List[Str], expand_tabs: Bool, initial_tab: Bool, tabsize: Str?, suppress_blank_empty: Bool, paginate: Bool, recursive: Bool, no_deref: Bool, new_file: Bool, unidirectional: Bool, ignore_name_case: Bool, no_ignore_name_case: Bool, exclude: List[Str], exclude_from: List[Str], starting_file: Str?, from_file: Str?, to_file: Str?, ignore_case: Bool, ignore_tab: Bool, trailing_space: Bool, space_change: Bool, all_space: Bool, ignore_blank: Bool, ignore_matching: List[Str], text: Bool, strip_cr: Bool, minimal: Bool, speed: Bool, horizon: Str?, color: Str?, palette: Str?, help: Bool, version: Bool, files: List[Str]}

# Everything a comparison needs besides the two names. `switches` is the option
# text GNU diff repeats in the `diff -r a/f b/f` line it prints before the
# output of each file compared inside a directory.
type Settings = {opts: Options, rules: gnudiff.Rules, fmt: diffrender.Format, templates: diffrender.Templates, labels: List[Str], switches: Str, patterns: List[Str], style: Str, fold_names: Bool}

# One merged-file option as typed.
type FormatEvent = {name: Str, value: Str}

# A directory entry name with its case-folded sort key.
type Folded = {key: Str, name: Str}

# One operand or directory entry after `stat`. `kind` is the `fs.stat` kind,
# plus "stdin" for `-` and "absent" for a missing path that `-N` treats as an
# empty file or directory. `identity` is device and inode, so a directory
# reached again through a symlink is recognised. `target` is a symlink's
# destination under `--no-dereference`.
type Probe = {name: Str, kind: Str, size: Int, identity: Str, target: Str, mtime_ns: Int, rdev: Int}

# A device number as "(major, minor)" in the Linux encoding.
pure device_numbers(rdev: Int) -> Str {
  let major = rdev / 256 % 4096 + rdev / 4294967296 * 4096
  let minor = rdev % 256 + rdev / 4096 % 1048576 * 256
  f"({major}, {minor})"
}

# GNU's wording for a file type in "File A is a T while file B is a T".
pure type_name(item: Probe) -> Str {
  match item.kind {
    "file" => if item.size == 0 { "regular empty file" } else { "regular file" }
    "dir" => "directory"
    "symlink" => "symbolic link"
    "fifo" => "fifo"
    "socket" => "socket"
    "block" => "block special file"
    "char" => "character special file"
    _ => "weird file"
  }
}

pure join(dir: Str, name: Str) -> Str {
  if dir == "" or dir.ends_with("/") { dir + name } else { dir + "/" + name }
}

pure worse(first: Int, second: Int) -> Int {
  if first > second { first } else { second }
}

# Whether any byte of `word` is one of the ASCII characters in `chars`.
pure has_any(word: Str, chars: Str) -> Bool {
  let raw = bytes.from_text(word)
  let members = bytes.from_text(chars)
  for index in range(raw.len()) {
    if raw[index..index + 1] in members { return true }
  }
  false
}

# Quote one option word the way the shell would need it: untouched when it has
# no shell metacharacters, double-quoted when only a single quote forces
# quoting, otherwise single-quoted with embedded quotes spliced in.
pure shell_quote(word: Str) -> Str {
  let special = word == "" or word == "{" or word == "}" or word.starts_with("#") or word.starts_with("~") or has_any(word, SHELL_SPECIAL)
  if !special { return word }
  if word.find("'") != null and !has_any(word, "$`\"\\") { return "\"" + word + "\"" }
  "'" + word.replace("'", with: "'\\''") + "'"
}

# Whether a possibly abbreviated long option takes a value in the next word.
pure long_takes_value(word: Str) -> Bool {
  let name = word.byte_slice(2)
  if name in VALUE_LONGS { return true }
  for candidate in VALUE_LONGS {
    if candidate.starts_with(name) { return true }
  }
  false
}

# The command-line options as typed, for the per-file `diff` line.
pure switch_text(argv: List[Str]) -> Str {
  var text = ""
  var index = 0
  while index < argv.len() {
    let arg = argv[index]
    if arg == "--" {
      text += " --"
      break
    }
    if arg.starts_with("--") {
      text += " " + shell_quote(arg)
      if arg.find("=") == null and long_takes_value(arg) {
        index += 1
        text += " " + shell_quote(argv.get(index) ?? "")
      }
    } else if arg.starts_with("-") and arg != "-" {
      text += " " + shell_quote(arg)
      let letters = arg.byte_slice(1)
      for at in range(letters.byte_len()) {
        let letter = letters.byte_slice(at, length: 1)
        if VALUE_LETTERS.find(letter) != null {
          if at == letters.byte_len() - 1 {
            index += 1
            text += " " + shell_quote(argv.get(index) ?? "")
          }
          break
        }
      }
    }
    index += 1
  }
  text
}

# A count option as GNU reads it with `strtoimax`: optional leading white
# space and sign, then decimal digits that saturate beyond the range. The empty
# string reads as zero; anything else left over is invalid (null).
pure parse_count(text: Str) -> Int? {
  if text == "" { return 0 }
  var digits = text
  while digits.starts_with(" ") or digits.starts_with("\t") { digits = digits.byte_slice(1) }
  var negative = false
  if digits.starts_with("+") {
    digits = digits.byte_slice(1)
  } else if digits.starts_with("-") {
    negative = true
    digits = digits.byte_slice(1)
  }
  if digits == "" { return null }
  let raw = bytes.from_text(digits)
  for index in range(raw.len()) {
    let value = raw.byte_at(index) ?? 0
    if value < 48 or value > 57 { return null }
  }
  var magnitude = 4611686018427387904
  if digits.byte_len() <= 18 {
    let number = digits.parse_int()
    if let Ok(found) = number { magnitude = found }
  }
  if negative { -magnitude } else { magnitude }
}

pure excluded(patterns: List[Str], name: Str) -> Bool {
  let raw = bytes.from_text(name)
  for pattern in patterns { if search.glob(pattern, raw) { return true } }
  false
}

# The name used in a report about the compared files: a `--label` replaces it
# (printed as given), otherwise the name is quoted when it needs it.
proc shown(cfg: Settings, side: Int, name: Str) [env] -> Str {
  if cfg.labels.len() > side { return cfg.labels[side] }
  gnu.quote_maybe(name)
}

# Non-ASCII bytes are printable only in a UTF-8 locale.
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

# A file name as it appears in the `diff -r a/f b/f` line: C-style double
# quotes with backslash escapes, but only when a space, control byte, quote or
# unprintable byte makes quoting necessary.
pure c_name(name: Str, utf8: Bool) -> Str {
  let raw = bytes.from_text(name)
  var out = ""
  var quoted = false
  var at = 0
  while at < raw.len() {
    let value = raw.byte_at(at) ?? 0
    var piece = ""
    var width = 1
    if value >= 128 {
      let need = if value >= 240 and value <= 244 { 4 } else if value >= 224 { 3 } else if value >= 194 { 2 } else { 0 }
      let char = if utf8 and need > 0 and at + need <= raw.len() { raw[at..at + need].utf8() ?? "" } else { "" }
      if char == "" {
        piece = f"\\{value / 64}{value / 8 % 8}{value % 8}"
        quoted = true
      } else {
        piece = char
        width = need
      }
    } else if value == 34 {
      piece = "\\\""
      quoted = true
    } else if value == 92 {
      piece = "\\\\"
    } else if value == 32 {
      piece = " "
      quoted = true
    } else if value < 32 or value == 127 {
      quoted = true
      piece = match value {
        7 => "\\a"
        8 => "\\b"
        9 => "\\t"
        10 => "\\n"
        11 => "\\v"
        12 => "\\f"
        13 => "\\r"
        _ => f"\\{value / 64}{value / 8 % 8}{value % 8}"
      }
    } else {
      piece = raw[at..at + 1].utf8() ?? ""
    }
    out += piece
    at += width
  }
  if quoted { "\"" + out + "\"" } else { name }
}

# Like `c_name` for the first or second operand, with a label in its place.
proc line_name(cfg: Settings, side: Int, name: Str) [env] -> Str {
  c_name(if cfg.labels.len() > side { cfg.labels[side] } else { name }, utf8_locale())
}

proc probe(cfg: Settings, name: Str, absent_ok: Bool) [fs, error] -> Result[Probe, Error] {
  if name == "-" {
    let info = fs.stat(p"/dev/stdin", follow_symlinks: true)
    let moment = if let Ok(found) = info { found.mtime_ns } else { 0 }
    return Ok({name: name, kind: "stdin", size: 0, identity: "", target: "", mtime_ns: moment, rdev: 0})
  }
  match fs.stat(fp"{name}", follow_symlinks: !cfg.opts.no_deref) {
    Ok(info) => {
      var target = ""
      if info.kind == "symlink" { target = fp"{name}".readlink()?.display() }
      Ok({name: name, kind: info.kind, size: info.size, identity: f"{info.dev}:{info.ino}", target: target, mtime_ns: info.mtime_ns, rdev: info.rdev})
    }
    Err(failure) => if absent_ok and gnu.errno(failure) == 2 { Ok({name: name, kind: "absent", size: 0, identity: "", target: "", mtime_ns: 0, rdev: 0}) } else { Err(failure) }
  }
}

# Binary detection looks only at the first block of a file, as GNU does.
pure has_nul(data: Bytes) -> Bool {
  let head = if data.len() > 4096 { data[0..4096] } else { data }
  b"\0" in head
}

proc stamp(moment: Int, unified: Bool) [env, time, error] -> Str {
  let format = if unified { "%Y-%m-%d %H:%M:%S.%N %z" } else { "%a %b %e %H:%M:%S %Y" }
  time.format(moment, format) ?? f"{moment / 1000000000}"
}

# The two label lines of a unified or context diff. A `--label` is printed
# without a timestamp.
proc header(cfg: Settings, left: Probe, right: Probe) [env, time, error] -> Bytes {
  let unified = cfg.style == "unified"
  let marks = if unified { ["---", "+++"] } else { ["***", "---"] }
  var chunks: List[Bytes] = []
  for side in range(2) {
    let item = if side == 0 { left } else { right }
    var text = ""
    if cfg.labels.len() > side {
      text = f"{marks[side]} {cfg.labels[side]}"
    } else {
      let moment = if item.kind == "absent" { 0 } else { item.mtime_ns }
      text = f"{marks[side]} {c_name(item.name, utf8_locale())}\t{stamp(moment, unified)}"
    }
    chunks += [diffrender.paint(cfg.fmt, 1, bytes.from_text(text)), b"\n"]
  }
  bytes.concat(chunks)
}

# Compare two byte strings and print the result in the selected format.
# `entry` marks a comparison reached through a directory: those print the
# `diff` line first. Returns 0 (same), 1 (differ), or 2 (trouble).
proc compare_contents(cfg: Settings, left: Probe, right: Probe, left_data: Bytes, right_data: Bytes, entry: Bool) [fs, io, process, env, error, time] -> Int {
  let opts = cfg.opts
  let left_shown = shown(cfg, 0, left.name)
  let right_shown = shown(cfg, 1, right.name)
  let binary = !opts.text and (has_nul(left_data) or has_nul(right_data))
  # Side-by-side and merged-file output list the common lines even of
  # identical text files.
  let templates = cfg.templates
  let merged_shows_all = cfg.style == "ifdef" and templates.unchanged_group != "" and !(templates.unchanged_group == "%=" and templates.unchanged_line == "")
  let lists_common = ((cfg.style == "sdiff" and !opts.suppress_common) or merged_shows_all) and !opts.brief and !binary
  if left_data == right_data and !lists_common {
    if opts.report_same { gnu.write_text(f"Files {left_shown} and {right_shown} are identical\n") }
    return 0
  }
  if binary {
    if opts.brief { gnu.write_text(f"Files {left_shown} and {right_shown} differ\n") } else { gnu.write_text(f"Binary files {left_shown} and {right_shown} differ\n") }
    return 1
  }
  var first = gnudiff.split(left_data, opts.strip_cr)
  var second = gnudiff.split(right_data, opts.strip_cr)
  let first_open = first.open
  let second_open = second.open
  if cfg.style == "ed" {
    first = {lines: first.lines, open: false}
    second = {lines: second.lines, open: false}
  }
  let script = gnudiff.script(first, second, cfg.rules)
  if !diffrender.significant(first, second, script, cfg.fmt) {
    if cfg.style == "ed" and (first_open or second_open) { return missing_newline_trouble(left, right, first_open, second_open) }
    if lists_common {
      if entry { gnu.write_text(f"diff{cfg.switches} {line_name(cfg, 0, left.name)} {line_name(cfg, 1, right.name)}\n") }
      gnu.write_bytes(render_merged(cfg, first, second, script, b""))
    }
    if opts.report_same { gnu.write_text(f"Files {left_shown} and {right_shown} are identical\n") }
    return 0
  }
  if opts.brief {
    gnu.write_text(f"Files {left_shown} and {right_shown} differ\n")
    return 1
  }
  if entry { gnu.write_text(f"diff{cfg.switches} {line_name(cfg, 0, left.name)} {line_name(cfg, 1, right.name)}\n") }
  let top = if cfg.style == "unified" or cfg.style == "context" { header(cfg, left, right) } else { b"" }
  gnu.write_bytes(render_merged(cfg, first, second, script, top))
  if cfg.style == "ed" and (first_open or second_open) { return missing_newline_trouble(left, right, first_open, second_open) }
  1
}

# An ed script cannot say that a line lacks its newline: GNU treats the
# comparison as having failed after printing what it could.
proc missing_newline_trouble(left: Probe, right: Probe, first_open: Bool, second_open: Bool) [process, env] -> Int {
  if first_open { gnu.error(f"{left.name}: No newline at end of file\n") }
  if second_open { gnu.error(f"{right.name}: No newline at end of file\n") }
  2
}

pure render_merged(cfg: Settings, first: gnudiff.Source, second: gnudiff.Source, script: List[gnudiff.Change], top: Bytes) -> Bytes {
  if cfg.style == "ifdef" { return diffrender.render_ifdef(first, second, script, cfg.fmt, cfg.templates) }
  diffrender.render(first, second, script, cfg.fmt, top)
}

error DeviceError = TooLarge(message: Str)

# The most a device operand may supply: comparing an endless device such as
# /dev/zero cannot finish, so it stops with an error rather than exhausting
# memory.
const DEVICE_LIMIT = 67108864

# Read a character or block device in chunks until it ends or passes the limit.
proc read_device(name: Str) [fs, io, error] -> Result[Bytes, Error] {
  let source = tio.open_source(name)?
  var parts: List[Bytes] = []
  var total = 0
  while true {
    let chunk = tio.read_chunk(source, total, 65536)?
    if chunk.is_empty() { break }
    parts += [chunk]
    total += chunk.len()
    if total >= DEVICE_LIMIT { return Err(DeviceError.TooLarge(message: "input is unbounded or too large to compare")) }
  }
  Ok(bytes.concat(parts))
}

proc read_side(item: Probe) [fs, io, error] -> Result[Bytes, Error] {
  if item.kind == "absent" { return Ok(b"") }
  if item.kind == "char" or item.kind == "block" { return read_device(item.name) }
  gnu.read_operand(item.name)
}

# One directory side: its names in byte order (case-folded order under
# `--ignore-file-name-case`), minus excluded names. An absent side (`-N`) is
# an empty directory.
proc entry_names(cfg: Settings, side: Probe) [fs, error] -> Result[List[Str], Error] {
  var names: List[Str] = []
  if side.kind == "absent" { return Ok(names) }
  for child in fs.children(fp"{side.name}", stat: false, ordered: true)? {
    if !excluded(cfg.patterns, child.name) { names += [child.name] }
  }
  if cfg.fold_names {
    var keyed: List[Folded] = []
    for name in names { keyed += [{key: name.lower(), name: name}] }
    var ordered: List[Str] = []
    for item in keyed |> sort-by .key { ordered += [item.name] }
    names = ordered
  }
  Ok(names)
}

pure fold_name(cfg: Settings, name: Str) -> Str {
  if cfg.fold_names { name.lower() } else { name }
}

# Compare two directories entry by entry in name order. Recursion into
# subdirectories happens in `compare_known` under `-r`. `trail` holds the
# identity pairs of the directories being compared above this one. `top` is
# true for the directories named on the command line, where `-S` applies.
proc diff_dirs(cfg: Settings, left: Probe, right: Probe, trail: List[Str], top: Bool) [fs, io, process, env, error, time] -> Int {
  let left_listing = entry_names(cfg, left)
  if let Err(failure) = left_listing { gnu.name_error(left.name, failure); return 2 }
  let right_listing = entry_names(cfg, right)
  if let Err(failure) = right_listing { gnu.name_error(right.name, failure); return 2 }
  let left_names = left_listing ?? []
  let right_names = right_listing ?? []
  var status = 0
  var left_at = 0
  var right_at = 0
  let start = cfg.opts.starting_file ?? ""
  while left_at < left_names.len() or right_at < right_names.len() {
    var left_name = ""
    var right_name = ""
    var in_left = false
    var in_right = false
    if left_at >= left_names.len() {
      right_name = right_names[right_at]
      in_right = true
    } else if right_at >= right_names.len() {
      left_name = left_names[left_at]
      in_left = true
    } else {
      let a = fold_name(cfg, left_names[left_at])
      let b = fold_name(cfg, right_names[right_at])
      if a == b {
        in_left = true
        in_right = true
        left_name = left_names[left_at]
        right_name = right_names[right_at]
      } else if a < b {
        left_name = left_names[left_at]
        in_left = true
      } else {
        right_name = right_names[right_at]
        in_right = true
      }
    }
    if in_left { left_at += 1 }
    if in_right { right_at += 1 }
    let name = if in_left { left_name } else { right_name }
    if top and start != "" and name < start { continue }
    if in_left and in_right {
      status = worse(status, compare_entry(cfg, join(left.name, left_name), join(right.name, right_name), trail, false))
    } else if in_left {
      if cfg.opts.new_file {
        status = worse(status, compare_entry(cfg, join(left.name, left_name), join(right.name, left_name), trail, true))
      } else {
        gnu.write_text(f"Only in {gnu.quote_maybe(left.name)}: {gnu.quote_maybe(left_name)}\n")
        status = worse(status, 1)
      }
    } else {
      if cfg.opts.new_file or cfg.opts.unidirectional {
        status = worse(status, compare_entry(cfg, join(left.name, right_name), join(right.name, right_name), trail, true))
      } else {
        gnu.write_text(f"Only in {gnu.quote_maybe(right.name)}: {gnu.quote_maybe(right_name)}\n")
        status = worse(status, 1)
      }
    }
  }
  status
}

# Compare two probed operands that are already past command-line directory
# redirection.
proc compare_known(cfg: Settings, left: Probe, right: Probe, entry: Bool, trail: List[Str]) [fs, io, process, env, error, time] -> Int {
  let left_dir = left.kind == "dir" or (left.kind == "absent" and right.kind == "dir")
  let right_dir = right.kind == "dir" or (right.kind == "absent" and left.kind == "dir")
  let left_shown = shown(cfg, 0, left.name)
  let right_shown = shown(cfg, 1, right.name)
  if left_dir and right_dir {
    if cfg.style == "ifdef" {
      gnu.error("-D option not supported with directories")
      return 2
    }
    if entry and !cfg.opts.recursive {
      gnu.write_text(f"Common subdirectories: {gnu.quote_maybe(left.name)} and {gnu.quote_maybe(right.name)}\n")
      return 0
    }
    let pair = left.identity + "|" + right.identity
    if pair in trail {
      gnu.error(f"{gnu.quote_maybe(left.name)}: recursive directory loop")
      return 2
    }
    return diff_dirs(cfg, left, right, trail + [pair], !entry)
  }
  if left_dir or right_dir {
    gnu.write_text(f"File {left_shown} is a {type_name(left)} while file {right_shown} is a {type_name(right)}\n")
    return 1
  }
  if left.kind == "symlink" or right.kind == "symlink" {
    if left.kind == "symlink" and right.kind == "symlink" {
      if left.target == right.target {
        if cfg.opts.report_same { gnu.write_text(f"Files {left_shown} and {right_shown} are identical\n") }
        return 0
      }
      gnu.write_text(f"Symbolic links {gnu.quote(left.name)} -> {gnu.quote(left.target)} and {gnu.quote(right.name)} -> {gnu.quote(right.target)} differ\n")
      return 1
    }
    gnu.write_text(f"File {left_shown} is a {type_name(left)} while file {right_shown} is a {type_name(right)}\n")
    return 1
  }
  let special = ["fifo", "socket", "block", "char"]
  if entry and (left.kind in special or right.kind in special) {
    # Inside a directory, special files are not read: device nodes are
    # compared by number and other kinds only by type.
    let left_kind = if left.kind == "absent" { right.kind } else { left.kind }
    let right_kind = if right.kind == "absent" { left.kind } else { right.kind }
    if left_kind == right_kind and (left_kind == "char" or left_kind == "block") {
      if left.rdev == right.rdev {
        if cfg.opts.report_same { gnu.write_text(f"Files {left_shown} and {right_shown} are identical\n") }
        return 0
      }
      let noun = if left_kind == "char" { "Character" } else { "Block" }
      gnu.write_text(f"{noun} special files {gnu.quote(left.name)} {device_numbers(left.rdev)} and {gnu.quote(right.name)} {device_numbers(right.rdev)} differ\n")
      return 1
    }
    let first_kind: Probe = {...left, kind: left_kind}
    let second_kind: Probe = {...right, kind: right_kind}
    gnu.write_text(f"File {left_shown} is a {type_name(first_kind)} while file {right_shown} is a {type_name(second_kind)}\n")
    return 1
  }
  if left.kind == "stdin" and right.kind == "stdin" {
    # Both operands are the same stream, equal to itself. It is read only for
    # the formats that list unchanged lines.
    let lists = ((cfg.style == "sdiff" and !cfg.opts.suppress_common) or cfg.style == "ifdef") and !cfg.opts.brief
    if !lists {
      if cfg.opts.report_same { gnu.write_text(f"Files {left_shown} and {right_shown} are identical\n") }
      return 0
    }
    let data = read_side(left)
    if let Err(failure) = data { gnu.name_error(left.name, failure); return 2 }
    return compare_contents(cfg, left, right, data ?? b"", data ?? b"", entry)
  }
  if left.identity != "" and left.identity == right.identity and cfg.style != "sdiff" and cfg.style != "ifdef" {
    if cfg.opts.report_same { gnu.write_text(f"Files {left_shown} and {right_shown} are identical\n") }
    return 0
  }
  let first = read_side(left)
  if let Err(failure) = first { gnu.name_error(left.name, failure); return 2 }
  let second = read_side(right)
  if let Err(failure) = second { gnu.name_error(right.name, failure); return 2 }
  compare_contents(cfg, left, right, first ?? b"", second ?? b"", entry)
}

# Probe both names of a directory entry and compare them. A name missing from
# one listing (`missing`) is an empty file under `-N`. Only the first failing
# name is reported, as GNU does for entries.
proc compare_entry(cfg: Settings, left_name: Str, right_name: Str, trail: List[Str], missing: Bool) [fs, io, process, env, error, time] -> Int {
  let left = probe(cfg, left_name, missing)
  if let Err(failure) = left { gnu.name_error(left_name, failure); return 2 }
  let right = probe(cfg, right_name, missing)
  if let Err(failure) = right { gnu.name_error(right_name, failure); return 2 }
  match left {
    Ok(first) => match right {
      Ok(second) => compare_known(cfg, first, second, true, trail)
      Err(_) => 2
    }
    Err(_) => 2
  }
}

# Command-line operands: a directory against a file compares the file with the
# same-named file inside the directory, as `cp` and `mv` resolve a target.
proc compare_operands(cfg: Settings, left_name: Str, right_name: Str) [fs, io, process, env, error, time] -> Int {
  let left = probe(cfg, left_name, cfg.opts.new_file or cfg.opts.unidirectional)
  let right = probe(cfg, right_name, cfg.opts.new_file)
  var failed = false
  if let Err(failure) = left { gnu.name_error(left_name, failure); failed = true }
  if let Err(failure) = right {
    # The same name is reported once.
    if !(failed and left_name == right_name) { gnu.name_error(right_name, failure) }
    failed = true
  }
  if failed { return 2 }
  let blank: Probe = {name: "", kind: "absent", size: 0, identity: "", target: "", mtime_ns: 0, rdev: 0}
  let first = match left { Ok(item) => item, Err(_) => blank }
  let second = match right { Ok(item) => item, Err(_) => blank }
  if first.kind == "absent" and second.kind == "absent" {
    if left_name == right_name {
      gnu.error(gnu.quote_maybe(left_name))
    } else {
      gnu.error(f"{gnu.quote_maybe(left_name)}: No such file or directory")
      gnu.error(f"{gnu.quote_maybe(right_name)}: No such file or directory")
    }
    return 2
  }
  let first_dir = first.kind == "dir"
  let second_dir = second.kind == "dir"
  if first_dir and second_dir and first.identity == second.identity {
    if cfg.style == "ifdef" {
      gnu.error("-D option not supported with directories")
      return 2
    }
    if cfg.style != "sdiff" or cfg.opts.suppress_common or cfg.opts.brief { return 0 }
    if left_name == right_name {
      # Side-by-side output names every entry of a directory compared with
      # itself as present in only one of them.
      let names = entry_names(cfg, first)
      if let Err(failure) = names { gnu.name_error(first.name, failure); return 2 }
      for name in names ?? [] { gnu.write_text(f"Only in {gnu.quote_maybe(first.name)}: {gnu.quote_maybe(name)}\n") }
      return 1
    }
  }
  # An absent first operand against a directory is read as a file.
  if first.kind == "absent" and second_dir {
    gnu.error(f"{gnu.quote_maybe(right_name)}: Is a directory")
    return 2
  }
  if first_dir == second_dir { return compare_known(cfg, first, second, false, []) }
  let directory = if first_dir { first } else { second }
  let other = if first_dir { second } else { first }
  if other.kind == "stdin" { gnu.error("cannot compare '-' to a directory"); return 2 }
  if other.kind == "absent" { return compare_known(cfg, first, second, false, []) }
  let inside = join(directory.name, fp"{other.name}".name())
  match probe(cfg, inside, cfg.opts.new_file) {
    Ok(moved) => if first_dir { compare_known(cfg, moved, second, false, []) } else { compare_known(cfg, first, moved, false, []) }
    Err(failure) => { gnu.name_error(inside, failure); 2 }
  }
}

# The GNU regular-expression diagnostic for a malformed pattern, or null when
# the pattern is well formed. The checks cover the mistakes people make:
# unterminated brackets, groups and intervals, bad ranges and class names, and
# a trailing backslash.
pure regex_problem(pattern: Str) -> Str? {
  let raw = bytes.from_text(pattern)
  let size = raw.len()
  let classes = ["alpha", "digit", "alnum", "upper", "lower", "space", "blank", "punct", "print", "graph", "cntrl", "xdigit"]
  var at = 0
  var depth = 0
  while at < size {
    let value = raw.byte_at(at) ?? 0
    if value == 92 {
      if at + 1 >= size { return "Trailing backslash" }
      let next = raw.byte_at(at + 1) ?? 0
      if next == 40 {
        depth += 1
      } else if next == 41 {
        if depth == 0 { return "Unmatched ) or \\)" }
        depth -= 1
      } else if next == 123 {
        var close = at + 2
        var found = false
        while close + 1 < size {
          if raw.byte_at(close) == 92 and raw.byte_at(close + 1) == 125 {
            found = true
            break
          }
          close += 1
        }
        if !found { return "Unmatched \\{" }
        let text = raw[at + 2..close].utf8() ?? ""
        let parts = text.split(",")
        if parts.len() > 2 { return "Invalid content of \\{\\}" }
        var low = 0
        var high = -1
        if parts[0] != "" {
          let number = parts[0].parse_int()
          if let Ok(found_low) = number { low = found_low } else { return "Invalid content of \\{\\}" }
        } else if parts.len() == 1 {
          return "Invalid content of \\{\\}"
        }
        if parts.len() == 2 and parts[1] != "" {
          let number = parts[1].parse_int()
          if let Ok(found_high) = number { high = found_high } else { return "Invalid content of \\{\\}" }
        } else if parts.len() == 1 {
          high = low
        }
        if low > 32767 or high > 32767 { return "Regular expression too big" }
        if high >= 0 and low > high { return "Invalid content of \\{\\}" }
        at = close + 2
        continue
      }
      at += 2
      continue
    }
    if value == 91 {
      let opener = at
      var cursor = at + 1
      if raw.byte_at(cursor) == 94 { cursor += 1 }
      if raw.byte_at(cursor) == 93 { cursor += 1 }
      var closed = false
      while cursor < size {
        let inner = raw.byte_at(cursor) ?? 0
        if inner == 93 {
          closed = true
          break
        }
        if inner == 91 and cursor + 1 < size and (raw.byte_at(cursor + 1) == 58 or raw.byte_at(cursor + 1) == 46 or raw.byte_at(cursor + 1) == 61) {
          let kind = raw.byte_at(cursor + 1) ?? 0
          var end = cursor + 2
          var shut = false
          while end + 1 < size {
            if raw.byte_at(end) == kind and raw.byte_at(end + 1) == 93 {
              shut = true
              break
            }
            end += 1
          }
          if !shut { return "Unmatched [, [^, [:, [., or [=" }
          let word = raw[cursor + 2..end].utf8() ?? ""
          if kind == 58 and !(word in classes) { return "Invalid character class name" }
          cursor = end + 2
          continue
        }
        if raw.byte_at(cursor + 1) == 45 and cursor + 2 < size and raw.byte_at(cursor + 2) != 93 {
          let high = raw.byte_at(cursor + 2) ?? 0
          if inner > high { return "Invalid range end" }
          cursor += 3
          continue
        }
        cursor += 1
      }
      if !closed {
        let bare = opener + 1 >= size or (opener + 2 >= size and raw.byte_at(opener + 1) == 94)
        return if bare { "Invalid regular expression" } else { "Unmatched [, [^, [:, [., or [=" }
      }
      at = cursor + 1
      continue
    }
    at += 1
  }
  if depth > 0 { return "Unmatched ( or \\(" }
  null
}

# Every long option name, for resolving an abbreviation in the typed words.
const FORMAT_LONGS = ["ifdef", "line-format", "old-line-format", "new-line-format", "unchanged-line-format", "old-group-format", "new-group-format", "changed-group-format", "unchanged-group-format"]

# The merged-file options in the order they were typed: canonical name (`-D`
# for the short and long forms of `--ifdef`) and value. Abbreviated long names
# resolve to the one option they can mean.
pure format_events(argv: List[Str]) -> List[FormatEvent] {
  var events: List[FormatEvent] = []
  var index = 0
  while index < argv.len() {
    let arg = argv[index]
    if arg == "--" { break }
    if arg.starts_with("--") {
      let body = arg.byte_slice(2)
      let equals = body.find("=")
      let name = if equals == null { body } else { body.byte_slice(0, length: equals ?? 0) }
      var resolved = ""
      var candidates: List[Str] = []
      for known in FORMAT_LONGS + VALUE_LONGS + OTHER_LONGS + ["digits"] {
        if known == name { resolved = known }
        if known.starts_with(name) and !(known in candidates) { candidates += [known] }
      }
      if resolved == "" and candidates.len() == 1 { resolved = candidates[0] }
      let takes = resolved in VALUE_LONGS
      if resolved in FORMAT_LONGS {
        var value = ""
        if equals != null {
          value = body.byte_slice((equals ?? 0) + 1)
        } else {
          index += 1
          value = argv.get(index) ?? ""
        }
        events += [{name: if resolved == "ifdef" { "-D" } else { "--" + resolved }, value: value}]
      } else if takes and equals == null {
        index += 1
      }
    } else if arg.starts_with("-") and arg != "-" {
      let letters = arg.byte_slice(1)
      for at in range(letters.byte_len()) {
        let letter = letters.byte_slice(at, length: 1)
        if VALUE_LETTERS.find(letter) != null {
          var value = letters.byte_slice(at + 1)
          if value == "" {
            index += 1
            value = argv.get(index) ?? ""
          }
          if letter == "D" { events += [{name: "-D", value: value}] }
          break
        }
      }
    }
    index += 1
  }
  events
}

# The merged-file templates after applying the typed options in order. A
# format given twice with different values, or a `-D` default that clashes with
# an explicit group format, ends the run as GNU does.
proc merged_templates(events: List[FormatEvent]) [process, env] -> diffrender.Templates {
  var groups: List[Str?] = [null, null, null, null]
  var line: List[Str?] = [null, null, null]
  let group_names = ["--old-group-format", "--new-group-format", "--changed-group-format", "--unchanged-group-format"]
  let line_names = ["--old-line-format", "--new-line-format", "--unchanged-line-format"]
  for event in events {
    var texts: List[Str] = []
    var is_group = false
    var slots: List[Int] = []
    if event.name == "-D" {
      is_group = true
      slots = [0, 1, 2, 3]
      texts = [f"#ifndef {event.value}\n%<#endif /* ! {event.value} */\n", f"#ifdef {event.value}\n%>#endif /* {event.value} */\n", f"#ifndef {event.value}\n%<#else /* {event.value} */\n%>#endif /* {event.value} */\n", "%="]
    } else if event.name == "--line-format" {
      slots = [0, 1, 2]
      texts = [event.value, event.value, event.value]
    } else if event.name in group_names {
      is_group = true
      for position in range(group_names.len()) {
        if group_names[position] == event.name { slots = [position] }
      }
      texts = [event.value]
    } else {
      for position in range(line_names.len()) {
        if line_names[position] == event.name { slots = [position] }
      }
      texts = [event.value]
    }
    for position in range(slots.len()) {
      let slot = slots[position]
      let current = if is_group { groups[slot] } else { line[slot] }
      let text = texts[position]
      if current != null and current != text {
        gnu.error(f"conflicting {event.name} option value {locale_quote(text)}")
        eprint f"{gnu.prog()}: Try '{gnu.phrase()} --help' for more information."
        exit 2
      }
      if is_group { groups[slot] = text } else { line[slot] = text }
    }
  }
  # An unspecified old or new group falls back to the changed format, which
  # itself defaults to the old and new formats back to back.
  let old_group = groups[0] ?? groups[2] ?? "%<"
  let new_group = groups[1] ?? groups[2] ?? "%>"
  let changed_group = groups[2] ?? old_group + new_group
  {old_group: old_group, new_group: new_group, changed_group: changed_group, unchanged_group: groups[3] ?? "%=", old_line: line[0] ?? "%l\n", new_line: line[1] ?? "%l\n", unchanged_line: line[2] ?? "%l\n"}
}

# A value quoted the way GNU `quote` does in the current locale. In a UTF-8
# locale the curly quotes need no escape for an apostrophe inside the value.
proc locale_quote(text: Str) [env] -> Str {
  let quoted = gnu.quote_value(text)
  if quoted.starts_with("\u{2018}") { return quoted.replace("\\'", with: "'") }
  quoted
}

# GNU diff words its usage hint with the program prefix, unlike other GNU
# tools, and exits with status 2.
proc usage_trouble(message: Str) [process, env] -> Unit {
  gnu.error(message)
  eprint f"{gnu.prog()}: Try '{gnu.phrase()} --help' for more information."
  exit 2
}

# A count option, or the usage error GNU gives for a bad value.
proc count_option(text: Str, what: Str, minimum: Int) [process, env] -> Int {
  let value = parse_count(text)
  if value == null or (value ?? 0) < minimum { usage_trouble(f"invalid {what} {locale_quote(text)}") }
  value ?? 0
}

proc main(...argv: List[Str]) [fs, io, process, env, error, time] {
  let outcome = cli.applet(argv, {
    gnu: {status: 2},
    normal: {form: "--normal", default: false},
    unified: {form: "-u", default: false},
    unified_short: {form: "-U NUM", repeated: true},
    unified_long: {form: "--unified[=NUM]", repeated: true, optional_default: "3"},
    context_c: {form: "-c", default: false},
    context_short: {form: "-C NUM", repeated: true},
    context_long: {form: "--context[=NUM]", repeated: true, optional_default: "3"},
    digits: {form: "--digits NUM", kind: "Int", numeric: true, repeated: true},
    ed: {form: "-e --ed", default: false},
    rcs: {form: "-n --rcs", default: false},
    side: {form: "-y --side-by-side", default: false},
    ifdef: {form: "-D --ifdef NAME", repeated: true},
    line_format: {form: "--line-format FMT", repeated: true},
    old_line_format: {form: "--old-line-format FMT", repeated: true},
    new_line_format: {form: "--new-line-format FMT", repeated: true},
    unchanged_line_format: {form: "--unchanged-line-format FMT", repeated: true},
    old_group_format: {form: "--old-group-format FMT", repeated: true},
    new_group_format: {form: "--new-group-format FMT", repeated: true},
    changed_group_format: {form: "--changed-group-format FMT", repeated: true},
    unchanged_group_format: {form: "--unchanged-group-format FMT", repeated: true},
    width: {form: "-W --width NUM"},
    left_column: {form: "--left-column", default: false},
    suppress_common: {form: "--suppress-common-lines", default: false},
    brief: {form: "-q --brief", default: false},
    report_same: {form: "-s --report-identical-files", default: false},
    show_c: {form: "-p --show-c-function", default: false},
    show_function: {form: "-F --show-function-line RE", repeated: true},
    label: {form: "-L --label LABEL", repeated: true},
    expand_tabs: {form: "-t --expand-tabs", default: false},
    initial_tab: {form: "-T --initial-tab", default: false},
    tabsize: {form: "--tabsize NUM"},
    suppress_blank_empty: {form: "--suppress-blank-empty", default: false},
    paginate: {form: "-l --paginate", default: false},
    recursive: {form: "-r --recursive", default: false},
    no_deref: {form: "--no-dereference", default: false},
    new_file: {form: "-N --new-file", default: false},
    unidirectional: {form: "--unidirectional-new-file", default: false},
    ignore_name_case: {form: "--ignore-file-name-case", default: false},
    no_ignore_name_case: {form: "--no-ignore-file-name-case", default: false},
    exclude: {form: "-x --exclude PAT", repeated: true},
    exclude_from: {form: "-X --exclude-from FILE", repeated: true},
    starting_file: {form: "-S --starting-file FILE"},
    from_file: {form: "--from-file FILE1"},
    to_file: {form: "--to-file FILE2"},
    ignore_case: {form: "-i --ignore-case", default: false},
    ignore_tab: {form: "-E --ignore-tab-expansion", default: false},
    trailing_space: {form: "-Z --ignore-trailing-space", default: false},
    space_change: {form: "-b --ignore-space-change", default: false},
    all_space: {form: "-w --ignore-all-space", default: false},
    ignore_blank: {form: "-B --ignore-blank-lines", default: false},
    ignore_matching: {form: "-I --ignore-matching-lines RE", repeated: true},
    text: {form: "-a --text", default: false},
    strip_cr: {form: "--strip-trailing-cr", default: false},
    minimal: {form: "-d --minimal", default: false},
    speed: {form: "--speed-large-files", default: false},
    horizon: {form: "--horizon-lines NUM"},
    color: {form: "--color[=WHEN]", optional_default: "auto"},
    palette: {form: "--palette PALETTE"},
    help: {form: "--help", default: false, stop: true},
    version: {form: "-v --version", default: false, stop: true},
    files: {form: "...FILE"},
  })
  if let Err(failure) = outcome {
    eprint failure.message.replace("\nTry '", with: f"\n{gnu.prog()}: Try '")
    exit 2
  }
  let parsed = outcome?
  let opts: Options = parsed
  if opts.help { gnu.write_text(HELP); return }
  if opts.version { gnu.version("diff"); return }

  var styles: List[Str] = []
  if opts.normal { styles += ["normal"] }
  if opts.unified or !opts.unified_short.is_empty() or !opts.unified_long.is_empty() { styles += ["unified"] }
  if opts.context_c or !opts.context_short.is_empty() or !opts.context_long.is_empty() { styles += ["context"] }
  if opts.ed { styles += ["ed"] }
  if opts.rcs { styles += ["rcs"] }
  if opts.side { styles += ["sdiff"] }
  let merged = !opts.ifdef.is_empty() or !opts.line_format.is_empty() or !opts.old_line_format.is_empty() or !opts.new_line_format.is_empty() or !opts.unchanged_line_format.is_empty() or !opts.old_group_format.is_empty() or !opts.new_group_format.is_empty() or !opts.changed_group_format.is_empty() or !opts.unchanged_group_format.is_empty()
  if merged { styles += ["ifdef"] }
  if styles.len() > 1 { usage_trouble("conflicting output style options") }
  let style = if !styles.is_empty() { styles[0] } else if opts.show_c { "context" } else { "normal" }

  var context = -1
  for text in opts.unified_short + opts.unified_long + opts.context_short + opts.context_long {
    let value = count_option(text, "context length", 0)
    if value > context { context = value }
  }
  for value in opts.digits { if value > context { context = value } }
  if (opts.unified or opts.context_c) and context < 3 { context = 3 }
  if context < 0 { context = if style == "unified" or style == "context" { 3 } else { 0 } }

  var width = 130
  if let text = opts.width { width = count_option(text, "width", 1) }
  var tabsize = 8
  if let text = opts.tabsize { tabsize = count_option(text, "tabsize", 1) }
  var horizon = 0
  if let text = opts.horizon { horizon = count_option(text, "horizon length", 0) }
  # Lines of context are kept in the comparison alongside any requested horizon.
  if (style == "unified" or style == "context") and context > horizon { horizon = context }

  if opts.paginate { usage_trouble("pagination not supported on this host") }
  var colored = false
  if let choice = opts.color {
    if choice == "always" {
      colored = true
    } else if choice == "auto" {
      let term = env.get_or("TERM", "") ?? ""
      colored = unix.isatty(1) and term != "" and term != "dumb"
    } else if choice != "never" {
      usage_trouble(f"invalid color {locale_quote(choice)}")
    }
  }
  var palette = ["0", "1", "32", "31", "36"]
  let custom = if colored { opts.palette } else { null }
  if let text = custom {
    let names = ["rs", "hd", "ad", "de", "ln"]
    var broken = false
    for item in text.split(":") {
      if item == "" { continue }
      let equals = item.find("=")
      if equals == null {
        broken = true
        continue
      }
      let key = item.byte_slice(0, length: equals ?? 0)
      let value = item.byte_slice((equals ?? 0) + 1)
      var known = false
      for position in range(names.len()) {
        if names[position] == key {
          palette[position] = value
          known = true
        }
      }
      if !known {
        gnu.error(f"unrecognized prefix: {key}")
        broken = true
      }
    }
    # A bad palette is only a warning: output continues without colour.
    if broken {
      gnu.error("unparsable value for --palette")
      colored = false
    }
  }
  if opts.label.len() > 2 {
    gnu.error("too many file label options")
    exit 2
  }
  if opts.from_file != null and opts.to_file != null {
    gnu.error("--from-file and --to-file both specified")
    exit 2
  }

  var patterns = opts.exclude
  for listing in opts.exclude_from {
    let data = gnu.read_operand(listing)
    if let Err(failure) = data { gnu.name_error(listing, failure); exit 2 }
    for line in ((data ?? b"").utf8() ?? "").lines() {
      if line != "" { patterns += [line] }
    }
  }
  var function_patterns = opts.show_function
  if opts.show_c { function_patterns += ["^[[:alpha:]$_]"] }
  for pattern in opts.ignore_matching + function_patterns {
    let problem = regex_problem(pattern)
    if problem != null {
      gnu.error(f"'{pattern}': {problem ?? ""}")
      exit 2
    }
    if let Err(_) = regex.find_bytes(pattern, b"") {
      gnu.error(f"'{pattern}': Invalid regular expression")
      exit 2
    }
  }

  var fold_names = false
  for word in argv {
    if word == "--" { break }
    if word == "--ignore-file-name-case" { fold_names = true }
    if word == "--no-ignore-file-name-case" { fold_names = false }
  }

  let templates = merged_templates(format_events(argv))
  # GNU does not expand tabs for comparison when trailing white space is also
  # ignored: the combination compares like `-Z` alone.
  let rules: gnudiff.Rules = {utf8: utf8_locale(), tabsize: tabsize, ignore_case: opts.ignore_case, all_space: opts.all_space, space_change: opts.space_change, trailing_space: opts.trailing_space, expand_tabs: opts.ignore_tab and !opts.trailing_space, minimal: opts.minimal, speed_large_files: opts.speed, horizon: horizon}
  let fmt: diffrender.Format = {utf8: rules.utf8, style: style, context: context, initial_tab: opts.initial_tab, expand_tabs: opts.expand_tabs, tabsize: tabsize, suppress_blank_empty: opts.suppress_blank_empty, width: width, left_column: opts.left_column, suppress_common: opts.suppress_common, function_patterns: function_patterns, ignore_blank: opts.ignore_blank, blank_skips_space: opts.space_change or opts.all_space, patterns: opts.ignore_matching, palette: if colored { palette } else { [] }}
  let cfg: Settings = {opts: opts, rules: rules, fmt: fmt, templates: templates, labels: opts.label, switches: switch_text(argv), patterns: patterns, style: style, fold_names: fold_names}

  var pairs: List[List[Str]] = []
  if let from = opts.from_file {
    for operand in opts.files { pairs += [[from, operand]] }
  } else if let to = opts.to_file {
    for operand in opts.files { pairs += [[operand, to]] }
  } else {
    if opts.files.len() < 2 {
      var last = gnu.prog()
      for word in argv {
        if word != "--" { last = word }
      }
      if opts.files.len() == 1 { last = opts.files[0] }
      usage_trouble(f"missing operand after {locale_quote(last)}")
    }
    if opts.files.len() > 2 { usage_trouble(f"extra operand {locale_quote(opts.files[2])}") }
    pairs += [[opts.files[0], opts.files[1]]]
  }
  var status = 0
  for pair in pairs {
    status = worse(status, compare_operands(cfg, pair[0], pair[1]))
  }
  if status != 0 { exit status }
}
