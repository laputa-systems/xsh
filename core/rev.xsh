#!/bin/xsh
use lib.gnu
use lib.textio_a1 as tio

const USAGE = """
Usage:
 rev [options] [file ...]

Reverse lines characterwise.

Options:
 -0, --zero     use the NUL byte as line separator
 -h, --help     display this help
 -V, --version  display version
"""

type Reversed = {out: Bytes, rest: Bytes}

type RevOptions = {zero: Bool, help: Bool, version: Bool, files: List[Str]}

# The byte width of the UTF-8 character at `at`: its sequence length when the
# lead and continuation bytes are well formed, otherwise 1 so that an invalid
# byte stays a character of its own.
pure char_width(data: Bytes, at: Int) -> Int {
  let lead = data.byte_at(at) ?? 0
  let width = if lead >= 240 and lead <= 244 {
    4
  } else if lead >= 224 and lead <= 239 {
    3
  } else if lead >= 194 and lead <= 223 {
    2
  } else {
    1
  }

  for offset in range(1, width) {
    let next = data.byte_at(at + offset) ?? 0

    return 1 when next < 128 or next > 191
  }

  width
}

# Reverse the characters of one line; bytes that are not UTF-8 are kept whole.
pure reverse_line(content: Bytes) -> Bytes {
  if let Ok(text) = content.utf8() {
    return bytes.from_text(text.reverse())
  }

  var pieces: List[Bytes] = []
  var at = 0

  while at < content.len() {
    let width = char_width(content, at)
    pieces = [content[at..at + width], @pieces]
    at += width
  }

  bytes.concat(pieces)
}

# Reverse every complete line of `data`; the unterminated rest is returned
# unless `final`, in which case it is reversed too.
pure reverse_lines(data: Bytes, zero: Bool, final: Bool) -> Reversed {
  let ends = tio.line_ends(data, zero)
  var start = 0

  let pieces: List[Bytes] = collect {
    for end in ends {
      yield @[reverse_line(data[start..end - 1]), data[end - 1..end]]
      start = end
    }
  }

  let rest = data[start..]

  if final and ! rest.is_empty() {
    return {out: bytes.concat([@pieces, reverse_line(rest)]), rest: b""}
  }

  {out: bytes.concat(pieces), rest: rest}
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: RevOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      zero: {form: "-0 --zero", default: false},
      help: {form: "-h --help", default: false, stop: true},
      version: {form: "-V --version", default: false, stop: true},
      files: {form: "...FILE"},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }

  if opts.version {
    gnu.version("rev")
    return
  }

  var failed = false
  var rest = b""

  for name in if opts.files.is_empty() { ["-"] } else { opts.files } {
    guard let source = tio.open_source(name) else { |failure|
      gnu.error(f"cannot open {gnu.quote_maybe(name)}: {gnu.strerror(failure)}")
      failed = true
      continue
    }

    var offset = 0

    loop {
      guard let chunk = tio.read_chunk(source, offset) else { |failure|
        gnu.error(f"read error on {gnu.quote_maybe(name)}: {gnu.strerror(failure)}")
        failed = true
        break
      }

      break when chunk.is_empty()

      offset += chunk.len()

      let done = reverse_lines(bytes.concat([rest, chunk]), opts.zero, false)
      rest = done.rest
      gnu.write_bytes(done.out)
    }

    if ! rest.is_empty() {
      gnu.write_bytes(reverse_lines(rest, opts.zero, true).out)
      rest = b""
    }
  }

  if failed {
    exit 1
  }
}
