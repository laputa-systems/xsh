#!/bin/xsh
use lib.gnu

const USAGE = """Usage: more [OPTIONS] FILE...
Display the contents of a text file

  -d, --silent       display help instead of ringing the bell when an illegal key is pressed
  -f, --logical      count logical lines, rather than screen lines
  -e, --exit-on-eof  exit on end of the last file
  -l, --no-pause     do not pause after any line containing a ^L (form feed)
  -p, --print-over   do not scroll, clear the screen and display text
  -c, --clean-print  do not scroll, display text and clean line ends
  -s, --squeeze      squeeze multiple blank lines into one
  -u, --plain        suppress underlining
  -n, --lines N      the number of lines per screenful
      --number N     same as --lines
  -F, --from-line N  start displaying each file at line number N
  -P, --pattern STR  display each file from the first line containing STR
      --help         display this help and exit
      --version      output version information and exit

The terminal is read a line at a time, so each command ends with RETURN:
RETURN scrolls a line, SPACE (then RETURN) a screenful, b goes back a
screenful, k back a line, /TEXT searches, n repeats the search and q quits.
"""

const COMMANDS_HELP = "RETURN: next line, SPACE: next screenful, b: previous screenful, k: previous line, /TEXT: search, n: search again, q: quit"

type MoreOptions = {
  silent: Bool,
  logical: Bool,
  exit_on_eof: Bool,
  no_pause: Bool,
  print_over: Bool,
  clean_print: Bool,
  squeeze: Bool,
  plain: Bool,
  lines: Str?,
  number: Str?,
  from_line: Str?,
  pattern: Str?,
  help: Bool,
  version: Bool,
  files: List[Str],
}

# How the screen is laid out: lines per screenful (0 means from the window),
# the window width, and whether commands can be read at all.
type Geometry = {rows: Int, cols: Int, interactive: Bool}

# One file or the standard input: its display name, a name for the prompt,
# and its lines as raw bytes.
type Source = {label: Str, lines: List[Bytes], size: Int}

# What the user asked for after a screen: quit everything or go on.
type Outcome = {quit: Bool}

const ESC = "\u{1b}"

proc count_option(text: Str?, option: Str) [process, env] -> Int {
  guard let given = text else {
    return 0
  }

  let parsed = given.parse_uint() ?? -1

  if parsed < 0 or parsed > 65535 {
    gnu.usage_error(f"invalid argument {gnu.quote_value(given)} for '{option}'")
  }

  parsed
}

proc window() [process, env] -> Geometry {
  var rows = 0
  var cols = 0

  if let Ok(size) = unix.window_size(1) {
    rows = size.rows
    cols = size.cols
  }

  if rows == 0 {
    rows = (env.get_or("LINES", "") ?? "").parse_uint() ?? 0
  }

  if cols == 0 {
    cols = (env.get_or("COLUMNS", "") ?? "").parse_uint() ?? 0
  }

  {rows: if rows > 0 { rows } else { 24 }, cols: if cols > 0 { cols } else { 80 }, interactive: true}
}

# Underline (`_` backspace char) and bold (`char` backspace `char`) overstrikes
# reduced to the plain character, for -u.
pure strip_overstrike(line: Bytes) -> Bytes {
  var out: List[Int] = []
  let total = line.len()
  var at = 0

  while at < total {
    let byte = line.byte_at(at) ?? 0
    let next = line.byte_at(at + 1)
    let after = line.byte_at(at + 2)

    if next == 8 and after != null and (byte == 95 or byte == after) {
      at += 2
    } else {
      out += [byte]
      at += 1
    }
  }

  bytes.from_ints(out) ?? line
}

pure line_text(line: Bytes) -> Str {
  line.utf8() ?? ""
}

# Screen rows a line takes: one when counting logical lines, else as many as
# its width needs, with tabs to the next multiple of 8.
pure line_rows(line: Bytes, cols: Int, logical: Bool) -> Int {
  return 1 when logical

  var width = 0

  for char in line_text(line) {
    if char == "\t" {
      width = (width / 8 + 1) * 8
    } else {
      width += 1
    }
  }

  if width <= cols { 1 } else { (width + cols - 1) / cols }
}

pure is_blank(line: Bytes) -> Bool {
  line.trim().len() == 0
}

# The index of the first line at or after `from` containing `needle`, or -1.
pure find_line(lines: List[Bytes], from: Int, needle: Str) -> Int {
  var index = from

  while index < lines.len() {
    return index when line_text(lines[index]).find(needle) != null

    index += 1
  }

  -1
}

# Lines of one screenful from `top`: the bytes to print, the index after the
# last line shown, and how many source lines were skipped as squeezed blanks.
type Screen = {text: Bytes, next: Int}

pure build_screen(opts: MoreOptions, lines: List[Bytes], top: Int, capacity: Int, cols: Int) -> Screen {
  var chunks: List[Bytes] = []
  var used = 0
  var index = top
  let end = if opts.clean_print { b"\x1b[K\n" } else { b"\n" }

  while index < lines.len() and used < capacity {
    let line = lines[index]
    index += 1

    if opts.squeeze and index > 1 and is_blank(line) and is_blank(lines[index - 2]) {
      continue
    }

    let shown = if opts.plain { strip_overstrike(line) } else { line }
    chunks += [shown, end]
    used += line_rows(line, cols, opts.logical)

    break when ! opts.no_pause and line_text(line).find("\u{c}") != null
  }

  {text: bytes.concat(chunks), next: index}
}

pure percent(lines: List[Bytes], upto: Int, size: Int) -> Int {
  var bytes_seen = 0

  for index in range(upto) {
    bytes_seen += lines[index].len() + 1
  }

  if size == 0 { 100 } else { bytes_seen * 100 / size }
}

proc prompt(opts: MoreOptions, source: Source, next_name: Str?, eof: Bool, upto: Int) -> Str {
  var progress = ""

  if eof {
    progress = if let following = next_name { f" (Next file: {following})" } else { " (END)" }
  } else if source.label != ":" {
    let done = percent(source.lines, upto, source.size)
    progress = if done >= 100 { " (END)" } else { f" ({done}%)" }
  }

  let text = f"{source.label}{progress}{if opts.silent { "[Press space to continue, 'q' to quit.]" } else { "" }}"
  f"{ESC}[7m{text}{ESC}[0m"
}

proc show_notice(text: Str) [process, env, io, error] {
  gnu.write_text(f"\r{ESC}[7m{text} (press RETURN){ESC}[0m")
  flush_output()
  let _ = wait_for_line()
}

proc flush_output() [process, env, io] {
  if let Err(failure) = io.flush_stdout() {
    gnu.write_failed(failure)
  }
}

# Commands come a line at a time; end of input quits.
proc wait_for_line() [io] -> Str? {
  match io.stdin_line() {
    Ok(text) => text
    Err(_) => null
  }
}

# One source on an interactive terminal. Returns whether the user quit.
proc page(opts: MoreOptions, source: Source, next_name: Str?, geometry: Geometry, show_header: Bool, last: Bool) [process, env, io, error] -> Outcome {
  let from_line = count_option(opts.from_line, "--from-line")
  let lines_per_screen = if let given = opts.lines { count_option(given, "--lines") } else { count_option(opts.number, "--number") }
  let capacity_full = if lines_per_screen > 0 { lines_per_screen } else if geometry.rows > 1 { geometry.rows - 1 } else { 1 }
  var top = if from_line > 0 { from_line - 1 } else { 0 }
  var pattern = opts.pattern ?? ""

  if top >= source.lines.len() and source.lines.len() > 0 {
    show_notice(f"Cannot seek to line number {top + 1}")
    top = 0
  }

  if pattern != "" {
    let found = find_line(source.lines, top, pattern)

    if found >= 0 {
      top = found
    } else {
      show_notice("Pattern not found")
      pattern = ""
    }
  }

  var header_rows = 0

  if show_header {
    gnu.write_text(f":::::::::::::::\n{source.label}\n:::::::::::::::\n")
    header_rows = 3
  }

  var first = true

  while true {
    let capacity = if first and capacity_full > header_rows { capacity_full - header_rows } else { capacity_full }
    first = false

    if opts.print_over {
      gnu.write_text(f"{ESC}[H{ESC}[2J")
    } else if opts.clean_print {
      gnu.write_text(f"{ESC}[H")
    }

    let screen = build_screen(opts, source.lines, top, capacity, geometry.cols)
    gnu.write_bytes(screen.text)

    let eof = screen.next >= source.lines.len()

    if eof and last and opts.exit_on_eof {
      flush_output()
      return {quit: true}
    }

    gnu.write_text(prompt(opts, source, next_name, eof, screen.next))
    flush_output()

    var wrong = false
    var redraw = false

    while ! redraw {
      guard let typed = wait_for_line() else {
        return {quit: true}
      }

      let key = typed.byte_slice(0, length: 1)
      gnu.write_text(f"{ESC}[A{ESC}[2K\r")

      if key == "q" or key == "Q" {
        gnu.write_text("\n")
        return {quit: true}
      } else if typed == "" or key == "j" {
        return {quit: false} when eof

        top += 1
        redraw = true
      } else if key == " " or key == "f" or key == "z" {
        return {quit: false} when eof

        top = screen.next
        redraw = true
      } else if key == "b" {
        top = if top > capacity_full { top - capacity_full } else { 0 }
        redraw = true
      } else if key == "k" {
        top = if top > 0 { top - 1 } else { 0 }
        redraw = true
      } else if key == "/" or key == "n" {
        if key == "/" and typed.byte_len() > 1 {
          pattern = typed.byte_slice(1)
        }

        let found = if pattern == "" { -1 } else { find_line(source.lines, top + 1, pattern) }

        if found >= 0 {
          top = found
          redraw = true
        } else {
          gnu.write_text(f"{ESC}[7mPattern not found{ESC}[0m")
          flush_output()
        }
      } else if key == "h" {
        gnu.write_text(f"{ESC}[7m{COMMANDS_HELP}{ESC}[0m")
        flush_output()
      } else {
        gnu.write_text(if opts.silent { f"{ESC}[7m[Press 'h' for instructions.]{ESC}[0m" } else { "\u{7}" })
        flush_output()
        wrong = true
      }
    }

    gnu.write_text(f"{ESC}[0m")
  }

  {quit: false}
}

# Everything printed straight through (no terminal to page on): the from-line
# and pattern choose the start, -s squeezes, a header names each of several files.
proc dump(opts: MoreOptions, source: Source, show_header: Bool) [process, env, io] {
  let from_line = count_option(opts.from_line, "--from-line")
  var top = if from_line > 0 { from_line - 1 } else { 0 }

  if let pattern = opts.pattern {
    let found = find_line(source.lines, top, pattern)

    if found >= 0 {
      top = found
    }
  }

  if show_header {
    gnu.write_text(f"::::::::::::::\n{source.label}\n::::::::::::::\n")
  }

  # -c and -p only repaint a terminal; a pipe gets the text alone.
  let plain = {...opts, clean_print: false, print_over: false}
  let screen = build_screen(plain, source.lines, top, source.lines.len() + 1, 80)
  gnu.write_bytes(screen.text)
}

proc load(name: Str) [fs, process, env, error, io] -> Source? {
  if name == "-" {
    let data = io.stdin_bytes()?
    return {label: ":", lines: data.lines(), size: data.len()}
  }

  let file = Path(name)

  match fs.stat(file) {
    Ok(info) => {
      if info.kind == "dir" {
        gnu.error(f"{gnu.quote(name)} is a directory.")
        return null
      }
    }
    Err(failure) => {
      gnu.cannot("open", name, failure)
      return null
    }
  }

  match file.read_bytes() {
    Ok(data) => {
      {label: name, lines: data.lines(), size: data.len()}
    }
    Err(failure) => {
      gnu.cannot("open", name, failure)
      null
    }
  }
}

proc main(...argv: List[Str]) [process, env, error, io, fs] {
  let opts: MoreOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      silent: {form: "-d --silent", default: false},
      logical: {form: "-f --logical", default: false},
      exit_on_eof: {form: "-e --exit-on-eof", default: false},
      no_pause: {form: "-l --no-pause", default: false},
      print_over: {form: "-p --print-over", default: false},
      clean_print: {form: "-c --clean-print", default: false},
      squeeze: {form: "-s --squeeze", default: false},
      plain: {form: "-u --plain", default: false},
      lines: {form: "-n --lines N"},
      number: {form: "--number N"},
      from_line: {form: "-F --from-line N"},
      pattern: {form: "-P --pattern PATTERN"},
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
    gnu.version("more")
    return
  }

  # Validate the numbers before any terminal work.
  let _ = count_option(opts.lines, "--lines")
  let _ = count_option(opts.number, "--number")
  let _ = count_option(opts.from_line, "--from-line")

  let names = if opts.files.len() == 0 { ["-"] } else { opts.files }
  let stdin_terminal = unix.isatty(0)

  if opts.files.len() == 0 and stdin_terminal {
    gnu.usage_error("bad usage")
  }

  # Paging needs a terminal to show screens on and commands to read from; a
  # pipe on standard input has neither a way to ask for more (no unbuffered
  # reads yet), so its text is printed in full.
  let geometry = window()
  let paging = unix.isatty(1) and stdin_terminal
  let several = names.len() > 1

  for index in range(names.len()) {
    guard let source = load(names[index]) else {
      continue
    }

    if paging {
      let next_name: Str? = if index + 1 < names.len() { names[index + 1] } else { null }
      let outcome = page(opts, source, next_name, geometry, several, index + 1 == names.len())

      break when outcome.quit
    } else {
      dump(opts, source, several)
    }
  }

  flush_output()
}
