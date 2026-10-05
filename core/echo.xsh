#!/bin/xsh
use lib.gnu

const USAGE = """Usage: echo [SHORT-OPTION]... [STRING]...
  or:  echo LONG-OPTION
Echo the STRING(s) to standard output.

  -n             do not output the trailing newline
  -e             enable interpretation of backslash escapes
  -E             disable interpretation of backslash escapes (default)
      --help     display this help and exit
      --version  output version information and exit

If -e is in effect, the following sequences are recognized:

  \\\\      backslash
  \\a      alert (BEL)
  \\b      backspace
  \\c      produce no further output
  \\e      escape
  \\f      form feed
  \\n      new line
  \\r      carriage return
  \\t      horizontal tab
  \\v      vertical tab
  \\0NNN   byte with octal value NNN (1 to 3 digits)
  \\xHH    byte with hexadecimal value HH (1 to 2 digits)

NOTE: your shell may have its own version of echo, which usually supersedes
the version described here.  Please refer to your shell's documentation
for details about the options it supports.
"""

type Expanded = {data: Bytes, stop: Bool}

pure octal_digit(byte: Int) -> Bool {
  byte >= 48 and byte <= 55
}

pure hex_value(byte: Int) -> Int {
  return byte - 48 when byte >= 48 and byte <= 57
  return byte - 87 when byte >= 97 and byte <= 102
  return byte - 55 when byte >= 65 and byte <= 70

  -1
}

pure simple_escape(byte: Int) -> Int {
  return 7 when byte == 97
  return 8 when byte == 98
  return 27 when byte == 101
  return 12 when byte == 102
  return 10 when byte == 110
  return 13 when byte == 114
  return 9 when byte == 116
  return 11 when byte == 118
  return 92 when byte == 92

  -1
}

# Expand the backslash escapes GNU echo -e knows. Octal and hex values wrap to
# one byte, and an unrecognized sequence keeps its backslash.
proc expand(text: Str) [error] -> Result[Expanded] {
  let raw = bytes.from_text(text)
  let total = raw.len()
  var out: List[Int] = []
  var at = 0

  while at < total {
    let byte = raw.byte_at(at) ?? 0
    at += 1

    if byte != 92 {
      out += [byte]
      continue
    }

    let next = if at < total { raw.byte_at(at) ?? 0 } else { -1 }
    let simple = simple_escape(next)

    if next == 99 {
      return Ok({data: bytes.from_ints(out)?, stop: true})
    } else if simple >= 0 {
      out += [simple]
      at += 1
    } else if next == 48 or (next >= 49 and next <= 55) {
      var value = 0
      var used = 0
      var limit = if next == 48 { 4 } else { 3 }
      at += if next == 48 { 1 } else { 0 }
      limit -= if next == 48 { 1 } else { 0 }

      while used < limit and at < total and octal_digit(raw.byte_at(at) ?? 0) {
        value = value * 8 + (raw.byte_at(at) ?? 48) - 48
        at += 1
        used += 1
      }

      out += [value % 256]
    } else if next == 120 {
      var value = 0
      var used = 0

      while used < 2 and at + 1 + used < total and hex_value(raw.byte_at(at + 1 + used) ?? 0) >= 0 {
        value = value * 16 + hex_value(raw.byte_at(at + 1 + used) ?? 0)
        used += 1
      }

      if used == 0 {
        out += [92]
      } else {
        out += [value]
        at += 1 + used
      }
    } else {
      out += [92]
    }
  }

  Ok({data: bytes.from_ints(out)?, stop: false})
}

# GNU echo's own option scan: leading arguments made only of n, e, and E after
# one dash are options; the first other argument and everything after it is text.
proc main(...argv: List[Str]) [process, env, error, io] {
  var posix = false

  if let Ok(_) = env.get("POSIXLY_CORRECT") {
    posix = true
  }

  if argv.len() == 1 and ! posix {
    if argv[0] == "--help" {
      gnu.help(USAGE)
      return
    }

    if argv[0] == "--version" {
      gnu.version("echo")
      return
    }
  }

  var escapes = false
  var newline = true
  var first = 0

  if ! posix or (! argv.is_empty() and argv[0] == "-n") {
    while first < argv.len() {
      let word = argv[first]

      break when ! word.starts_with("-") or word == "-"
      break when ! rx"^-[neE]+$".matches(word)

      let flags = word.byte_slice(1)

      for index in range(flags.byte_len()) {
        let flag = flags.byte_slice(index, length: 1)

        if flag == "e" {
          escapes = true
        } else if flag == "E" {
          escapes = false
        } else {
          newline = false
        }
      }

      first += 1
    }
  }

  let words = argv[first..argv.len()]

  if ! (escapes or posix) {
    gnu.write_text(words.join(" ") + (if newline { "\n" } else { "" }))
    return
  }

  var stopped = false

  let parts: List[Bytes] = collect {
    for index in range(words.len()) {
      yield b" " when index > 0

      let piece = expand(words[index])?
      yield piece.data

      if piece.stop {
        stopped = true
        break
      }
    }

    yield b"\n" when newline and ! stopped
  }

  gnu.write_bytes(bytes.concat(parts))
}
