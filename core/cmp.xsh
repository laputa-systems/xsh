#!/bin/xsh
use lib.gnu
use lib.diffutils
use lib.textio_a1 as tio

const HELP = """Usage: cmp [OPTION]... FILE1 [FILE2 [SKIP1 [SKIP2]]]
Compare two files byte by byte.

The optional SKIP1 and SKIP2 specify the number of bytes to skip
at the beginning of each file (zero by default).

Mandatory arguments to long options are mandatory for short options too.
  -b, --print-bytes          print differing bytes
  -i, --ignore-initial=SKIP         skip first SKIP bytes of both inputs
  -i, --ignore-initial=SKIP1:SKIP2  skip first SKIP1 bytes of FILE1 and
                                      first SKIP2 bytes of FILE2
  -l, --verbose              output byte numbers and differing byte values
  -n, --bytes=LIMIT          compare at most LIMIT bytes
  -s, --quiet, --silent      suppress all normal output
      --help                 display this help and exit
  -v, --version              output version information and exit

SKIP values may be followed by the following multiplicative suffixes:
kB 1000, K 1024, MB 1,000,000, M 1,048,576,
GB 1,000,000,000, G 1,073,741,824, and so on for T, P, E, Z, Y.

If a FILE is '-' or missing, read standard input.
Exit status is 0 if inputs are the same, 1 if different, 2 if trouble.

Report bugs to: bug-diffutils@gnu.org
GNU diffutils home page: <https://www.gnu.org/software/diffutils/>
General help using GNU software: <https://www.gnu.org/gethelp/>
"""

# Long option names, for resolving the abbreviations typed on the command line
# when option values are checked in the order they were given.
const LONGS = ["print-bytes", "ignore-initial", "verbose", "bytes", "quiet", "silent", "help", "version"]

# How many bytes one comparison step reads from each file.
const CHUNK = 65536

# The largest byte offset cmp works with.
const LARGEST = 9223372036854775807

type Options = {print_bytes: Bool, ignore_initial: Str?, list: Bool, bytes: Str?, silent: Bool, help: Bool, version: Bool, files: List[Str]}

# An option whose value or combination is checked in command-line order:
# kind `l`, `s`, `n` or `i` and the value text.
type Event = {kind: Str, value: Str}

# Report a usage problem the way cmp words it, with the program prefix on the
# hint, and end with status 2.
proc trouble(message: Str) [process, env] -> Unit {
  gnu.error(message)
  eprint f"{gnu.prog()}: Try '{gnu.phrase()} --help' for more information."
  exit 2
}

# The options that carry values or exclusions, in the order typed.
pure option_events(argv: List[Str]) -> List[Event] {
  var events: List[Event] = []
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
      for known in LONGS {
        if known == name { resolved = known }
        if known.starts_with(name) { candidates += [known] }
      }
      if resolved == "" and candidates.len() == 1 { resolved = candidates[0] }
      if resolved == "verbose" {
        events += [{kind: "l", value: ""}]
      } else if resolved == "quiet" or resolved == "silent" {
        events += [{kind: "s", value: ""}]
      } else if resolved == "bytes" or resolved == "ignore-initial" {
        var value = ""
        if equals != null {
          value = body.byte_slice((equals ?? 0) + 1)
        } else {
          index += 1
          value = argv.get(index) ?? ""
        }
        events += [{kind: if resolved == "bytes" { "n" } else { "i" }, value: value}]
      }
    } else if arg.starts_with("-") and arg != "-" {
      let letters = arg.byte_slice(1)
      for at in range(letters.byte_len()) {
        let letter = letters.byte_slice(at, length: 1)
        if letter == "l" {
          events += [{kind: "l", value: ""}]
        } else if letter == "s" {
          events += [{kind: "s", value: ""}]
        } else if letter == "n" or letter == "i" {
          var value = letters.byte_slice(at + 1)
          if value == "" {
            index += 1
            value = argv.get(index) ?? ""
          }
          events += [{kind: letter, value: value}]
          break
        }
      }
    }
    index += 1
  }
  events
}

type Initial = {first: Int, second: Int, bad: Str?}

# The skip counts of an `-i` value: `SKIP` for both files or `SKIP1:SKIP2`.
# The error text is the part GNU cmp quotes, or null when well formed.
pure parse_initial(text: Str) -> Initial {
  let head = diffutils.parse_size(text)
  if head == null { return {first: 0, second: 0, bad: text} }
  let found = head ?? {value: 0, rest: ""}
  if found.rest == "" { return {first: found.value, second: found.value, bad: null} }
  if !found.rest.starts_with(":") { return {first: 0, second: 0, bad: text} }
  let tail = found.rest.byte_slice(1)
  let second = diffutils.parse_size(tail)
  if second == null { return {first: 0, second: 0, bad: tail} }
  let other = second ?? {value: 0, rest: ""}
  if other.rest != "" { return {first: 0, second: 0, bad: tail} }
  {first: found.value, second: other.value, bad: null}
}

# The byte count of `text` as a skip operand, or null when it is not one.
pure parse_skip(text: Str) -> Int? {
  let parsed = diffutils.parse_size(text)
  if parsed == null { return null }
  let found = parsed ?? {value: 0, rest: "x"}
  if found.rest != "" { return null }
  found.value
}

pure digits_of(value: Int) -> Int {
  var width = 1
  var rest = value
  while rest >= 10 {
    rest = rest / 10
    width += 1
  }
  width
}

# The device and inode identity of an operand that exists, "" for stdin.
proc identity_of(name: Str) [fs] -> Str {
  if name == "-" { return "" }
  let info = fs.stat(fp"{name}", follow_symlinks: true)
  if let Ok(found) = info { return f"{found.dev}:{found.ino}" }
  ""
}

proc main(...argv: List[Str]) [fs, io, process, env, error] {
  let outcome = cli.applet(argv, {
    gnu: {status: 2},
    print_bytes: {form: "-b -c --print-bytes", default: false},
    ignore_initial: {form: "-i --ignore-initial SKIP"},
    list: {form: "-l --verbose", default: false},
    bytes: {form: "-n --bytes LIMIT"},
    silent: {form: "-s --quiet --silent", default: false},
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
  if opts.version { gnu.version("cmp"); return }

  var listed = false
  var quiet = false
  var skip_first = 0
  var skip_second = 0
  var limit = LARGEST
  var limited = false
  for event in option_events(argv) {
    if event.kind == "l" {
      if quiet { trouble("options -l and -s are incompatible") }
      listed = true
    } else if event.kind == "s" {
      if listed { trouble("options -l and -s are incompatible") }
      quiet = true
    } else if event.kind == "n" {
      let count = parse_skip(event.value)
      if count == null { trouble(f"invalid --bytes value {gnu.quote_value(event.value)}") }
      limit = count ?? 0
      limited = true
    } else {
      let skips = parse_initial(event.value)
      if let bad = skips.bad { trouble(f"invalid --ignore-initial value {gnu.quote_value(bad)}") }
      skip_first = skips.first
      skip_second = skips.second
    }
  }

  var last = gnu.prog()
  for word in argv {
    if word != "--" { last = word }
  }
  if opts.files.is_empty() { trouble(f"missing operand after {gnu.quote(last)}") }
  let left = opts.files[0]
  let right = opts.files.get(1) ?? "-"
  if opts.files.len() > 2 {
    let count = parse_skip(opts.files[2])
    if count == null { trouble(f"invalid --ignore-initial value {gnu.quote_value(opts.files[2])}") }
    skip_first = count ?? 0
  }
  if opts.files.len() > 3 {
    let count = parse_skip(opts.files[3])
    if count == null { trouble(f"invalid --ignore-initial value {gnu.quote_value(opts.files[3])}") }
    skip_second = count ?? 0
  }
  if opts.files.len() > 4 { trouble(f"extra operand {gnu.quote(opts.files[4])}") }

  # Open phase: a missing or unreadable file ends the run at the first one.
  var sources: List[tio.Source] = []
  for operand in [left, right] {
    if operand == "" {
      if !quiet { gnu.error("'': No such file or directory") }
      exit 2
    }
    let found = tio.open_source(operand)
    if let Err(failure) = found {
      if !quiet { gnu.name_error(operand, failure) }
      exit 2
    }
    let source = found ?? {name: operand, path: p"/dev/null", mode: "whole", kind: 0, size: 0}
    if source.mode != "stdin" and source.kind != 4 {
      let check = bytes.read_at(source.path, 0, 0)
      if let Err(failure) = check {
        if !quiet { gnu.name_error(operand, failure) }
        exit 2
      }
    }
    sources += [source]
  }
  let first = sources[0]
  let second = sources[1]
  if first.mode == "stdin" and second.mode == "stdin" { return }
  if first.mode != "stdin" and identity_of(left) == identity_of(right) and skip_first == skip_second { return }

  # The width of byte numbers in `-l` output: wide enough for the largest
  # offset that can be compared, judged from the regular files.
  var widest = limit
  var regular = false
  for position in range(2) {
    let source = sources[position]
    if source.kind == 8 {
      regular = true
      let skip = if position == 0 { skip_first } else { skip_second }
      var room = source.size - skip
      if room < 0 { room = 0 }
      if room < widest { widest = room }
    }
  }
  let width = if limited or regular { digits_of(widest) } else { 20 }

  if first.mode == "stdin" { diffutils.skip_stdin(skip_first)? }
  if second.mode == "stdin" { diffutils.skip_stdin(skip_second)? }

  var offset = 0
  var lines = 0
  var ended_line = true
  var different = false
  while offset < limit {
    let count = if limit - offset < CHUNK { limit - offset } else { CHUNK }
    let left_result = tio.read_chunk(first, skip_first + offset, count)
    if let Err(failure) = left_result {
      if !quiet { gnu.name_error(left, failure) }
      exit 2
    }
    let right_result = tio.read_chunk(second, skip_second + offset, count)
    if let Err(failure) = right_result {
      if !quiet { gnu.name_error(right, failure) }
      exit 2
    }
    let a = left_result?
    let b = right_result?
    let shared = if a.len() < b.len() { a.len() } else { b.len() }
    let head_a = a[0..shared]
    let head_b = b[0..shared]
    if head_a != head_b {
      different = true
      if quiet { exit 1 }
      if listed {
        var rows: List[Str] = []
        for index in range(shared) {
          let av = head_a.byte_at(index) ?? 0
          let bv = head_b.byte_at(index) ?? 0
          if av == bv { continue }
          let number = diffutils.pad(f"{offset + index + 1}", width)
          if opts.print_bytes {
            rows += [f"{number} {diffutils.octal(av)} {diffutils.pad_right(diffutils.display_byte(av), 4)} {diffutils.octal(bv)} {diffutils.display_byte(bv)}"]
          } else {
            rows += [f"{number} {diffutils.octal(av)} {diffutils.octal(bv)}"]
          }
        }
        gnu.write_text(rows.join("\n") + "\n")
      } else {
        let found = head_a.compare(head_b)
        let place = offset + found.byte
        let before = if found.byte > 1 { diffutils.newline_count(head_a[0..found.byte - 1]) } else { 0 }
        let line = lines + before + 1
        if opts.print_bytes {
          gnu.write_text(f"{left} {right} differ: byte {place}, line {line} is {diffutils.octal(found.left)} {diffutils.display_byte(found.left)} {diffutils.octal(found.right)} {diffutils.display_byte(found.right)}\n")
        } else {
          gnu.write_text(f"{left} {right} differ: char {place}, line {line}\n")
        }
        exit 1
      }
    }
    if a.len() != b.len() {
      if quiet { exit 1 }
      let shorter = if a.len() < b.len() { left } else { right }
      let total = offset + shared
      if total == 0 {
        gnu.error(f"EOF on {gnu.quote(shorter)} which is empty")
      } else if listed {
        gnu.error(f"EOF on {gnu.quote(shorter)} after byte {total}")
      } else {
        let seen = lines + diffutils.newline_count(head_a)
        let closed = if shared > 0 { head_a.ends_with(b"\n") } else { ended_line }
        if closed {
          gnu.error(f"EOF on {gnu.quote(shorter)} after byte {total}, line {seen}")
        } else {
          gnu.error(f"EOF on {gnu.quote(shorter)} after byte {total}, in line {seen + 1}")
        }
      }
      exit 1
    }
    if shared == 0 { break }
    lines += diffutils.newline_count(head_a)
    if shared > 0 { ended_line = head_a.ends_with(b"\n") }
    offset += shared
  }
  if different { exit 1 }
}
