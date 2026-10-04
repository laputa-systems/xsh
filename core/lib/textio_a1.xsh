##! Binary-safe operand reading shared by cat, tac, head, and tail.
##!
##! A `Source` is one input operand (`-` is standard input). Callers read it
##! with `read_chunk` until an empty chunk and report a failure by its errno:
##! `is_directory` marks the read error GNU words differently from an open
##! failure. Regular files are read in bounded chunks; standard input and
##! non-seekable files are read whole, because the runtime has no incremental
##! stdin or pipe read.

use gnu

## The size of one chunked read.
export const CHUNK = 65536

## `mode` is `stdin`, `file` (regular, chunked and seekable), `device`
## (character or block device read in chunks), or `whole` (read in one call).
export type Source = {name: Str, path: Path, mode: Str, size: Int}

## Position, append flag, and file identity of a standard descriptor.
export type Fd = {pos: Int, append: Bool, ino: Int, mnt: Int}

## Open an operand: resolve symlinks and classify the file. A failure is the
## operating-system error of the missing, looping, or unreachable name.
export proc open_source(name: Str) [fs, error] -> Result[Source] {
  return Ok({name: name, path: p"/dev/stdin", mode: "stdin", size: 0}) when name == "-"

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

  Ok({name: name, path: target, mode: mode, size: entry.size})
}

## The bytes of `source` from `offset`, at most `count` for chunked sources
## and everything for the others. An empty result is the end of the input.
export proc read_chunk(source: Source, offset: Int, count = CHUNK) [fs, error, io] -> Result[Bytes] {
  if source.mode == "stdin" {
    return if offset == 0 { io.stdin_bytes() } else { Ok(b"") }
  }

  if source.mode == "whole" {
    return if offset == 0 { source.path.read_bytes() } else { Ok(b"") }
  }

  if source.mode == "file" {
    return Ok(b"") when offset >= source.size

    let left = source.size - offset

    return bytes.read_at(source.path, offset, if left < count { left } else { count })
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

  for line in fp"/proc/self/fdinfo/{fd}".read_text()?.lines() {
    let parts = line.split(":")
    let name = parts.get(0) ?? ""
    let value = (parts.get(1) ?? "").trim().parse_int() ?? 0

    if name == "pos" {
      info = {...info, pos: value}
    } else if name == "flags" {
      info = {...info, append: value / 1024 % 2 == 1}
    } else if name == "ino" {
      info = {...info, ino: value}
    } else if name == "mnt_id" {
      info = {...info, mnt: value}
    }
  }

  info
}

## The resolved path of standard output when it is a regular file, else "".
export proc stdout_file() [fs, error] -> Str {
  guard let target = p"/dev/stdout".resolve() else {
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
## `stdout_file()`.
export proc is_unsafe_overwrite(source: Source, out: Str, written: Int) [fs, error] -> Result[Bool] {
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
