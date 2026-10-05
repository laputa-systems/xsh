##! Binary-safe operand reading shared by cat, tac, head, and tail.
##!
##! A `Source` is one input operand (`-` is standard input). Callers read it
##! with `read_chunk` until an empty chunk and report a failure by its errno:
##! `is_directory` marks the read error GNU words differently from an open
##! failure. Regular files are read in bounded chunks; standard input and
##! non-seekable files are read whole, because the runtime has no incremental
##! stdin or pipe read.

use gnu

## The largest count GNU `head` and `tail` clamp an overflowing number to.
export const MAX_COUNT = 9223372036854775807

## The size of one chunked read.
export const CHUNK = 65536

# Device input beyond this many bytes fails: stdout is only flushed when the
# applet exits, so output from an unbounded device (`/dev/zero`) could never
# be delivered and would only exhaust memory.
const DEVICE_LIMIT = 67108864

error InputError = Unbounded(message: Str)

## `mode` is `stdin`, `file` (nonempty regular file, chunked and seekable),
## `device` (character or block device read in chunks), or `whole` (read in
## one call). `kind` is the file type nibble of `st_mode` (8 regular, 4
## directory, 1 FIFO).
export type Source = {name: Str, path: Path, mode: Str, kind: Int, size: Int}

## Position, append flag, and file identity of a standard descriptor.
export type Fd = {pos: Int, append: Bool, ino: Int, mnt: Int}

## Open an operand: resolve symlinks and classify the file. A failure is the
## operating-system error of the missing, looping, or unreachable name.
export proc open_source(name: Str) [fs, error] -> Result[Source, Error] {
  return Ok({name: name, path: /dev/stdin, mode: "stdin", kind: 0, size: 0}) when name == "-"

  let target = fp"{name}".resolve()?
  let entry = target.metadata()?
  let kind = entry.mode / 4096 % 16
  let mode = if kind == 8 and entry.size > 0 {
    "file"
  } else if kind == 2 or kind == 6 {
    "device"
  } else {
    "whole"
  }

  Ok({name: name, path: target, mode: mode, kind: kind, size: entry.size})
}

## The bytes of `source` from `offset`, at most `count` for chunked sources
## and everything for the others. An empty result is the end of the input.
export proc read_chunk(source: Source, offset: Int, count = CHUNK) [fs, error, io] -> Result[Bytes, Error] {
  return if offset == 0 { io.stdin_bytes() } else { Ok(b"") } when source.mode == "stdin"

  if source.mode == "whole" {
    return if offset == 0 { source.path.read_bytes() } else { Ok(b"") }
  }

  if source.mode == "file" {
    return Ok(b"") when offset >= source.size

    let left = source.size - offset

    return bytes.read_at(source.path, offset, if left < count { left } else { count })
  }

  if offset >= DEVICE_LIMIT {
    return Err(InputError.Unbounded("input is unbounded and stdout is not flushed incrementally"))
  }

  match bytes.read_at(source.path, 0, count) {
    Ok(data) => Ok(data)
    Err(failure) => if failure.message.find("failed to fill") != null { Ok(b"") } else { Err(failure) }
  }
}

## Whether a read failed because the operand is a directory.
export pure is_directory(failure: Error) -> Bool {
  gnu.errno(failure) == 21
}

proc fd_info(fd: Int) [fs, error] -> Result[Fd] {
  var info = {pos: 0, append: false, ino: 0, mnt: 0}

  for line in fp"/proc/self/fdinfo/{fd}".lines()? {
    let parts = line.split(":")
    let name = parts.get(0) ?? ""
    let text = (parts.get(1) ?? "").trim()
    let value = text.parse_int() ?? 0

    if name == "pos" {
      info = {...info, pos: value}
    } else if name == "flags" {
      let mode = if text.byte_len() >= 4 { text.byte_slice(text.byte_len() - 4, length: 1) } else { "0" }
      info = {...info, append: mode in ["2", "3", "6", "7"]}
    } else if name == "ino" {
      info = {...info, ino: value}
    } else if name == "mnt_id" {
      info = {...info, mnt: value}
    }
  }

  info
}

## The resolved path of standard input (0) or output (1) when it is a regular
## file, else "".
export proc standard_file(fd: Int) [fs, error] -> Str {
  guard let target = fp"/dev/fd/{fd}".resolve() else {
    return ""
  }

  guard let entry = target.metadata() else {
    return ""
  }

  return "" when entry.mode / 4096 % 16 != 8

  target.display()
}

## GNU `cat` refuses to copy a nonempty regular file onto itself when reading
## would reach bytes the output wrote: the input offset is not past the output
## offset (or, for an appending output, not at the end of the file). Both
## offsets come from `/proc/self/fdinfo`; a file operand starts at offset 0 and
## `written` counts the bytes buffered for output so far. `out` is
## `standard_file(1)`.
export proc is_unsafe_overwrite(source: Source, out: Str, written: Int) [fs, error] -> Result[Bool, Error] {
  return Ok(false) when out == "" or (source.mode != "file" and source.mode != "stdin")

  guard let output = fd_info(1) else {
    return Ok(false)
  }

  let size = fp"{out}".metadata()?.size
  var position = 0

  if source.mode == "stdin" {
    guard let input = fd_info(0) else {
      return Ok(false)
    }

    return Ok(false) when input.ino != output.ino or input.mnt != output.mnt or size == 0

    position = input.pos
  } else if source.path.display() != out or source.size == 0 {
    return Ok(false)
  }

  Ok(if output.append { position < size } else { position < output.pos + written })
}

## Parse an unsigned count with a GNU size suffix (`b`, `K`, `KiB`, `kB`, `M`,
## ..., `Q`); an overflowing value clamps to `MAX_COUNT`. Returns null for
## text that is not a count.
export pure parse_count(text: Str) -> Int? {
  let parts = rx"^([0-9]*)(.*)$".captures(text)
  let digits = parts[1]
  let suffix = parts[2]

  return null when digits == "" and suffix == ""

  let number = if digits == "" { 1 } else if digits.byte_len() > 18 { MAX_COUNT } else { digits.parse_int() ?? 0 }
  var factor = 1

  if suffix == "b" {
    factor = 512
  } else if suffix != "" {
    let lead = suffix[0..1]
    let rest = suffix[1..]
    let known = lead in ["K", "k", "M", "m", "G", "T", "P", "E", "Z", "Y", "R", "Q"] and rest in ["", "iB", "B", "D"]

    return null when ! known

    let exponent = ("KMGTPEZYRQ".find(lead.upper()) ?? 0) + 1
    let base = if rest == "B" or rest == "D" { 1000 } else { 1024 }

    return MAX_COUNT when number > 0 and exponent > 6

    repeat exponent times {
      factor *= base
    }
  }

  return MAX_COUNT when number > MAX_COUNT / factor

  number * factor
}

## Offsets just past each line separator in `data`: newline by default, NUL
## with `zero`. `lines()` drops the CR of a CRLF, so a newline is recognized
## from the bytes after each line.
export pure line_ends(data: Bytes, zero: Bool) -> List[Int] {
  var ends: List[Int] = []

  if zero {
    for index in range(data.len()) {
      if data.byte_at(index) == 0 {
        ends += [index + 1]
      }
    }

    return ends
  }

  var position = 0

  for item in data.lines() {
    let end = position + item.len()
    let after = data.byte_at(end) ?? -1
    let crlf = after == 13 and (data.byte_at(end + 1) ?? -1) == 10

    if after == 10 or crlf {
      ends += [if crlf { end + 2 } else { end + 1 }]
    }

    position = end + (if crlf { 2 } else { 1 })
  }

  ends
}

## Drop GNU's undocumented `---presume-input-pipe` (it only selects the
## algorithm for seekable input, never the result); the cli grammar cannot
## declare an option name that starts with a dash.
export pure without_presume_pipe(argv: List[Str]) -> List[Str] {
  var kept: List[Str] = []
  var options = true

  for item in argv {
    if item == "--" {
      options = false
    }

    if ! (options and item == "---presume-input-pipe") {
      kept += [item]
    }
  }

  kept
}
