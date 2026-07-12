#!/usr/bin/env -S xsh --
# Bytes Inspect
# Inspect a file as bytes: hashes, encodings, text/binary hints, chunks, and comparisons.
# Usage: xsh showcase/bytes-inspect.xsh -- FILE [--compare FILE] [--chunk-size N]
# Example: xsh showcase/bytes-inspect.xsh -- artifact.bin --compare old.bin
type Opts = {file: List[Str], compare: Str, chunk_size: Int}

proc main(...argv: List[Str]) [fs, error] {
  let opts: Opts = cli.parse(
    argv,
    {
      file: {
        form: "...FILE",
        repeated: true,
        required: true,
      },
      compare: {
        form: "--compare FILE",
        default: "",
      },
      chunk_size: {
        form: "--chunk-size N",
        kind: "UInt",
        default: 512,
        min: 1,
      },
    },
  )?

  let file_arg = opts.file.get(0, "")
  let fp = fp"${file_arg}"
  let data = fp.read_bytes()?
  let size = data.len()
  print f"file:   ${fp.name()}"
  print f"size:   ${size} bytes"
  print f"sha1:   ${data.sha1().hex()}"
  print f"sha256: ${data.sha256().hex()}"
  print f"sha512: ${data.sha512().hex()}"
  print f"md5:    ${data.md5().hex()}"
  print f"base64: ${data.base64()}"
  print f"base32: ${data.base32()}"

  if size > 0 {
    let preview_len = if size < 32 { size } else { 32 }
    let preview = data.slice(0, preview_len)
    print f"hex:    ${preview.dump("hex-u8")}"
  }

  match data.utf8() {
    Ok(decoded) => print f"text:   ${decoded.count_lines()} lines"
    Err(_) => {
      let strings = data.strings(4)
      print f"binary: ${strings.len()} printable strings"
    }
  }

  var chunk_count = 0

  for _ in data.chunks(opts.chunk_size) {
    chunk_count += 1
  }

  print f"chunks: ${chunk_count} x ${opts.chunk_size} bytes"

  if opts.compare != "" {
    let other_fp = fp"${opts.compare}"
    let other = other_fp.read_bytes()?
    let cmp = data.compare(other)

    if cmp.equal {
      print "compare: identical"
    } else {
      print f"compare: differ at byte ${cmp.byte}"
    }
  }
}
