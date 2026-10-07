#!/bin/xsh
use lib.gnu
use lib.bytes_enc_dd as charset
use lib.textio_a1 as tio

type Options = {input: Str, output: Str, ibs: Int, obs: Int, cbs: Int, count: Int, skip: Int, seek: Int, count_bytes: Bool, skip_bytes: Bool, seek_bytes: Bool, conv: List[Str], status: Str, fullblock: Bool, bs: Int}
type Converted = {data: Bytes, truncated: Int}

pure amount(text: Str) -> Int? {
  let factors = text.split("x")
  var result = 1
  for factor in factors {
    let bytes_suffix = factor.ends_with("B") and rx"^[0-9]+B$".matches(factor)
    let raw = if bytes_suffix { factor.byte_slice(0, factor.byte_len() - 1) } else { factor }
    let number = if raw.ends_with("w") { (raw.byte_slice(0, raw.byte_len() - 1).parse_int() ?? -1) * 2 } else if raw.ends_with("c") { raw.byte_slice(0, raw.byte_len() - 1).parse_int() ?? -1 } else { tio.parse_count(raw) ?? -1 }
    return null when number < 0 or (number > 0 and result > 9223372036854775807 / number)
    result *= number
  }
  result
}

proc parse(argv: List[Str]) [process, env, io] -> Options {
  var opts: Options = {input: "-", output: "-", ibs: 512, obs: 512, cbs: 0, count: -1, skip: 0, seek: 0, count_bytes: false, skip_bytes: false, seek_bytes: false, conv: [], status: "default", fullblock: false, bs: 0}
  for arg in argv {
    let parts = arg.split("=")
    if parts.len() < 2 { gnu.usage_error(f"unrecognized operand {gnu.quote(arg)}") }
    let key = parts[0]
    let text = arg.byte_slice(key.byte_len() + 1)
    if key == "if" { opts = {...opts, input: text} } else if key == "of" { opts = {...opts, output: text} } else if key in ["ibs", "obs", "bs", "cbs", "count", "skip", "seek", "iseek", "oseek"] {
      let factors = text.split("x")
      for index in range(factors.len() - 1) {
        if factors[index] == "0" { gnu.error("warning: '0x' is a zero multiplier; use '00x' if that is intended") }
      }
      guard let n = amount(text) else { gnu.error(f"invalid number: {gnu.quote(text)}"); exit 1 }
      if n == 9223372036854775807 { gnu.error(f"invalid number: {gnu.quote(text)}: Value too large for defined data type"); exit 1 }
      if n == 0 and key in ["ibs", "obs", "bs", "cbs"] { gnu.error(f"invalid number: {gnu.quote(text)}"); exit 1 }
      let byte_count = [factor for factor in text.split("x") if rx"^[0-9]+B$".matches(factor)].len() > 0
      if key == "bs" { opts = {...opts, ibs: n, obs: n, bs: n} } else if key == "ibs" { opts = {...opts, ibs: n} } else if key == "obs" { opts = {...opts, obs: n} } else if key == "cbs" { opts = {...opts, cbs: n} } else if key == "count" { opts = {...opts, count: n, count_bytes: byte_count} } else if key in ["skip", "iseek"] { opts = {...opts, skip: n, skip_bytes: byte_count} } else { opts = {...opts, seek: n, seek_bytes: byte_count} }
    } else if key == "status" {
      if ! (text in ["none", "noxfer", "progress"]) { gnu.usage_error(f"invalid status level: {gnu.quote(text)}") }
      if text == "progress" { gnu.usage_error("unsupported status level progress: interruptible transfer reporting required") }
      opts = {...opts, status: text}
    } else if key in ["conv", "iflag", "oflag"] {
      for flag in text.split(",") {
        if key == "conv" {
          if flag in ["lcase", "ucase", "swab", "sync", "block", "unblock", "notrunc", "nocreat", "ascii", "ebcdic", "ibm"] { opts = {...opts, conv: opts.conv.extend([flag])} } else if flag in ["excl", "noerror", "fdatasync", "fsync", "sparse"] { gnu.usage_error(f"unsupported conversion {gnu.quote(flag)}: native descriptor support required") } else { gnu.usage_error(f"invalid conversion: {gnu.quote(flag)}") }
        } else {
          if key in ["iflag", "oflag"] and flag == "count_bytes" { opts = {...opts, count_bytes: true} } else if key == "iflag" and flag == "skip_bytes" { opts = {...opts, skip_bytes: true} } else if key == "iflag" and flag == "fullblock" { opts = {...opts, fullblock: true} } else if key == "oflag" and flag == "seek_bytes" { opts = {...opts, seek_bytes: true} } else if flag in ["direct", "directory", "dsync", "sync", "append", "nonblock", "noatime", "nocache", "nofollow", "nolinks", "cio", "text", "binary", "excl"] { gnu.usage_error(f"unsupported {key} {gnu.quote(flag)}: native descriptor support required") } else { gnu.usage_error(f"invalid {if key == "iflag" { "input" } else { "output" }} flag: {gnu.quote(flag)}") }
        }
      }
    } else { gnu.usage_error(f"unrecognized operand {gnu.quote(arg)}") }
  }
  if "lcase" in opts.conv and "ucase" in opts.conv { gnu.usage_error("cannot combine lcase and ucase") }
  if "block" in opts.conv and "unblock" in opts.conv { gnu.usage_error("cannot combine block and unblock") }
  if ("block" in opts.conv or "unblock" in opts.conv) and opts.cbs == 0 { gnu.usage_error("conversion block size must be specified") }
  let sets = [name for name in opts.conv if name in ["ascii", "ebcdic", "ibm"]]
  if sets.len() > 1 { gnu.usage_error("cannot combine any two of {ascii,ebcdic,ibm}") }
  if opts.cbs > 0 and "ascii" in opts.conv and ! ("block" in opts.conv or "unblock" in opts.conv) { opts = {...opts, conv: opts.conv.extend(["unblock"])} }
  if opts.cbs > 0 and ("ebcdic" in opts.conv or "ibm" in opts.conv) and ! ("block" in opts.conv or "unblock" in opts.conv) { opts = {...opts, conv: opts.conv.extend(["block"])} }
  if opts.bs > 0 { opts = {...opts, ibs: opts.bs, obs: opts.bs} }
  opts
}

pure encode_charset(data: Bytes, opts: Options) -> Result[Bytes] {
  return Ok(data) when ! ("ebcdic" in opts.conv or "ibm" in opts.conv)
  let direction = if "ibm" in opts.conv { "ibm" } else { "ebcdic" }
  bytes.from_ints([charset.character(data.byte_at(i) ?? 0, direction) for i in range(data.len())])
}

pure transform(records: List[Bytes], opts: Options) -> Result[Converted] {
  var pieces: List[Bytes] = []
  for block in records {
    var values = [block.byte_at(i) ?? 0 for i in range(block.len())]
    if "sync" in opts.conv and block.len() < opts.ibs {
      values += [if "block" in opts.conv or "unblock" in opts.conv { 32 } else { 0 } for _ in range(opts.ibs - block.len())]
    }
    if "swab" in opts.conv {
      for pair in range(values.len() / 2) {
        let first = values[pair * 2]
        values[pair * 2] = values[pair * 2 + 1]
        values[pair * 2 + 1] = first
      }
    }
    if "ascii" in opts.conv { values = [charset.character(value, "ascii") for value in values] }
    if "lcase" in opts.conv { values = [if value >= 65 and value <= 90 { value + 32 } else { value } for value in values] }
    if "ucase" in opts.conv { values = [if value >= 97 and value <= 122 { value - 32 } else { value } for value in values] }
    pieces += [bytes.from_ints(values)?]
  }
  let mapped = bytes.concat(pieces)
  if "block" in opts.conv {
    var out: List[Bytes] = []
    var start = 0
    var truncated = 0
    for at in range(mapped.len() + 1) {
      if at == mapped.len() or mapped.byte_at(at) == 10 {
        if at == mapped.len() and start == at { break }
        let length = at - start
        if length > opts.cbs { truncated += 1 }
        out += [mapped.slice(start, length: if length < opts.cbs { length } else { opts.cbs })]
        if length < opts.cbs { out += [bytes.from_ints([32 for _ in range(opts.cbs - length)])?] }
        start = at + 1
      }
    }
    return Ok({data: encode_charset(bytes.concat(out), opts)?, truncated: truncated})
  }
  if "unblock" in opts.conv {
    var out: List[Bytes] = []
    for block in mapped.chunks(opts.cbs) {
      var end = block.len()
      while end > 0 and block.byte_at(end - 1) == 32 { end -= 1 }
      out += [block[0..end], b"\n"]
    }
    return Ok({data: encode_charset(bytes.concat(out), opts)?, truncated: 0})
  }
  Ok({data: encode_charset(mapped, opts)?, truncated: 0})
}

pure human_size(size: Int, base: Int, binary: Bool) -> Str {
  var power = 1
  var exponent = 0
  while exponent < 6 and size / power >= base { power *= base; exponent += 1 }
  return f"{size} B" when exponent == 0
  let whole = size / power
  let suffix = ["", "k", "M", "G", "T", "P", "E"][exponent]
  let unit = if binary { (if exponent == 1 { "K" } else { suffix }) + "iB" } else { suffix + "B" }
  let tenth = (size % power * 10 + power / 2) / power
  return f"{whole + tenth / 10}.{tenth % 10} {unit}" when whole < 10
  f"{whole + (if size % power >= power / 2 { 1 } else { 0 })} {unit}"
}

type Copied = {size: Int, complete: Int, partial: Int}

proc write_fd_all(fd: Int, data: Bytes) [error, process] -> Result[Unit] {
  var offset = 0
  while offset < data.len() {
    let written = unix.write_fd(fd, data[offset..])?
    if written == 0 { fail "descriptor write made no progress" }
    offset += written
  }
  Ok()
}

## stdout is a byte stream even when it is redirected to a file, so a seek
## prefix must be emitted as zero bytes rather than represented by a hole.
proc prepare_stdout_seek(offset: Int) [error, io, env, process] {
  return when offset == 0
  var remaining = offset
  while remaining > 0 {
    let width = if remaining < 8192 { remaining } else { 8192 }
    gnu.write_bytes(bytes.zero(width)?)
    remaining -= width
  }
}

proc plain_copy(opts: Options, skip: Int, seek: Int, limit: Int) [fs, error, io, process, env] -> Result[Copied] {
  let dest = fp"{opts.output}"
  var output_fd: Int? = null
  if opts.output != "-" {
    if "nocreat" in opts.conv and ! dest.exists() { fail "No such file or directory" }
    if ! dest.exists() and ! ("nocreat" in opts.conv) { dest.write(b"")? }
    let kind = dest.metadata()?.mode / 4096 % 16
    if kind in [1, 8] { output_fd = unix.open_fd(dest, write: true, nonblock: false)? }
    if kind == 8 {
      if ! ("notrunc" in opts.conv) { dest.truncate(seek)? }
      if let fd = output_fd { let _ = unix.seek_fd(fd, seek)? }
    }
  }
  var source: tio.Source? = null
  if opts.input != "-" { source = tio.open_source(opts.input)? }
  var input_fd: Int? = null
  if let file = source {
    if file.kind in [1, 2, 6] { input_fd = unix.open_fd(file.path, nonblock: false)? }
    if file.mode == "file" and opts.output != "-" and dest.metadata()?.mode / 4096 % 16 == 8 {
      let size = file.path.metadata()?.size
      if skip > size { gnu.error(f"{gnu.quote(opts.input)}: cannot skip to specified offset") }
      let available = if size > skip { size - skip } else { 0 }
      let length = if available < limit { available } else { limit }
      if let fd = output_fd { unix.close_fd(fd)?; output_fd = null }
      let copied = bytes.copy_file(file.path, dest.resolve()?, source_offset: skip, dest_offset: seek, length: length, create: ! ("nocreat" in opts.conv))?
      return Ok({size: copied.bytes, complete: copied.bytes / opts.ibs, partial: if copied.bytes % opts.ibs == 0 { 0 } else { 1 }})
    }
  }
  if let fd = input_fd {
    var discarded = 0
    while discarded < skip {
      let want = if skip - discarded < opts.ibs { skip - discarded } else { opts.ibs }
      let block = unix.read_fd(fd, want)?
      break when block.is_empty()
      discarded += block.len()
    }
  } else if source == null {
    var discarded = 0
    while discarded < skip {
      let want = if skip - discarded < opts.ibs { skip - discarded } else { opts.ibs }
      let block = io.stdin_read(want)?
      break when block.is_empty()
      discarded += block.len()
    }
    if discarded < skip { gnu.error("'standard input': cannot skip to specified offset") }
  }
  var total = 0
  var complete = 0
  var partial = 0
  var output_chunks: List[Bytes] = []
  var output_length = 0
  loop {
    break when total >= limit or (opts.count >= 0 and ! opts.count_bytes and complete + partial >= opts.count)
    let want = if limit - total < opts.ibs { limit - total } else { opts.ibs }
    var block = b""
    if let fd = input_fd {
      block = unix.read_fd(fd, want)?
    } else if let file = source {
      block = if file.mode == "device" { bytes.read_at(file.path, 0, want)? } else { tio.read_chunk(file, skip + total, want)? }
      block = block.slice(0, length: want)
    } else {
      block = io.stdin_read(want)?
    }
    if opts.fullblock and ! block.is_empty() {
      var pieces = [block]
      var length = block.len()
      while length < want {
        let part = if let fd = input_fd { unix.read_fd(fd, want - length)? } else if let file = source {
          tio.read_chunk(file, skip + total + length, want - length)?
        } else { io.stdin_read(want - length)? }
        break when part.is_empty()
        pieces += [part]
        length += part.len()
      }
      block = bytes.concat(pieces)
    }
    break when block.is_empty()
    if opts.output == "-" { gnu.write_bytes(block); io.flush_stdout()? } else if let fd = output_fd {
      var rest = block
      loop {
        let needed = opts.obs - output_length
        if rest.len() < needed {
          output_chunks += [rest]
          output_length += rest.len()
          break
        }
        if output_chunks.is_empty() {
          write_fd_all(fd, rest.slice(0, length: opts.obs))?
        } else {
          output_chunks += [rest.slice(0, length: needed)]
          write_fd_all(fd, bytes.concat(output_chunks))?
          output_chunks = []
          output_length = 0
        }
        rest = rest[needed..]
        break when rest.is_empty()
      }
    } else { let _ = bytes.write_at(dest, seek + total, block, create: ! ("nocreat" in opts.conv))? }
    total += block.len()
    if block.len() == opts.ibs { complete += 1 } else { partial += 1 }
  }
  if let fd = output_fd {
    if output_length > 0 { write_fd_all(fd, bytes.concat(output_chunks))? }
  }
  if let fd = input_fd { unix.close_fd(fd)? }
  if let fd = output_fd { unix.close_fd(fd)? }
  Ok({size: total, complete: complete, partial: partial})
}

proc main(...argv: List[Str]) [fs, process, env, error, io, time] {
  if "--help" in argv { gnu.help("Usage: dd [OPERAND]...\nCopy a file, converting and formatting according to the operands.\n\nOperands:\n  if=FILE of=FILE bs=BYTES ibs=BYTES obs=BYTES cbs=BYTES\n  count=N skip=N seek=N status=none|noxfer|progress\n\nConversion options:\n  conv=ascii,ebcdic,ibm,block,unblock,lcase,ucase,swab,sync,notrunc,nocreat\n  iflag=count_bytes,skip_bytes,fullblock oflag=seek_bytes\nNative descriptor flags are not supported."); return }
  if "--version" in argv { gnu.version("dd"); return }
  let started = time.now()
  let opts = parse(argv)
  if opts.ibs > 67108864 or opts.obs > 67108864 or opts.cbs > 67108864 { gnu.error("memory exhausted"); exit 1 }
  if opts.skip > 0 and ! opts.skip_bytes and opts.skip > 9223372036854775807 / opts.ibs { gnu.error("Value too large for defined data type"); exit 1 }
  if opts.seek > 0 and ! opts.seek_bytes and opts.seek > 9223372036854775807 / opts.obs { gnu.error("Value too large for defined data type"); exit 1 }
  let skip = if opts.skip_bytes { opts.skip } else { opts.skip * opts.ibs }
  let seek = if opts.seek_bytes { opts.seek } else { opts.seek * opts.obs }
  if opts.output == "-" { prepare_stdout_seek(seek) }
  if opts.count > 0 and ! opts.count_bytes and opts.count > 9223372036854775807 / opts.ibs { gnu.error("count is too large"); exit 1 }
  let limit = if opts.count < 0 { 9223372036854775807 } else if opts.count_bytes { opts.count } else { opts.count * opts.ibs }
  if opts.input == "" or opts.output == "" { gnu.error("failed to open '': No such file or directory"); exit 1 }
  if opts.input != "-" {
    if let Err(failure) = tio.open_source(opts.input) { gnu.error(f"failed to open {gnu.quote(opts.input)}: {gnu.strerror(failure)}"); exit 1 }
  }
  if opts.output != "-" and "nocreat" in opts.conv and ! fp"{opts.output}".exists() { gnu.error(f"failed to open {gnu.quote(opts.output)}: No such file or directory"); exit 1 }
  let plain = [flag for flag in opts.conv if ! (flag in ["notrunc", "nocreat"])].is_empty()
  if plain {
    guard let copied = plain_copy(opts, skip, seek, limit) else { |failure| gnu.error(gnu.strerror(failure)); exit 1 }
    report(opts, copied.size, copied.complete, copied.partial, 0, started)
    return
  }
  var records: List[Bytes] = []
  var fifo_fd: Int? = null
  if opts.input != "-" {
    guard let source = tio.open_source(opts.input) else { |failure| gnu.error(f"failed to open {gnu.quote(opts.input)}: {gnu.strerror(failure)}"); exit 1 }
    if source.kind == 1 { fifo_fd = unix.open_fd(source.path, nonblock: false)? }
  }
  if let fd = fifo_fd {
    var discarded = 0
    while discarded < skip {
      let want = if skip - discarded < opts.ibs { skip - discarded } else { opts.ibs }
      let block = unix.read_fd(fd, want)?
      break when block.is_empty()
      discarded += block.len()
    }
    var remaining = limit
    var blocks = 0
    loop {
      break when remaining <= 0 or (opts.count >= 0 and ! opts.count_bytes and blocks >= opts.count)
      let want = if remaining < opts.ibs { remaining } else { opts.ibs }
      let first = unix.read_fd(fd, want)?
      break when first.is_empty()
      var pieces = [first]
      var length = first.len()
      if opts.fullblock {
        while length < want {
          let part = unix.read_fd(fd, want - length)?
          break when part.is_empty()
          pieces += [part]
          length += part.len()
        }
      }
      let block = bytes.concat(pieces)
      records += [block]
      remaining -= block.len()
      blocks += 1
    }
    unix.close_fd(fd)?
  } else if limit > 0 {
    if opts.input != "-" {
      guard let source = tio.open_source(opts.input) else { |failure| gnu.error(f"failed to open {gnu.quote(opts.input)}: {gnu.strerror(failure)}"); exit 1 }
      var chunks: List[Bytes] = []
      var at = skip
      var remaining = limit
      loop {
        let width = if remaining < opts.ibs { remaining } else { opts.ibs }
        break when width == 0
        guard let block = tio.read_chunk(source, at, width) else { |failure| gnu.error(f"error reading {gnu.quote(opts.input)}: {gnu.strerror(failure)}"); exit 1 }
        break when block.is_empty()
        let part = block.slice(0, length: remaining)
        chunks += [part]
        at += part.len()
        remaining -= part.len()
      }
      records = chunks
    } else {
      var discarded = 0
      while discarded < skip {
        let want = if skip - discarded < opts.ibs { skip - discarded } else { opts.ibs }
        guard let block = io.stdin_read(want) else { |failure| gnu.error(f"error reading 'standard input': {gnu.strerror(failure)}"); exit 1 }
        break when block.is_empty()
        discarded += block.len()
      }
      var read = 0
      var blocks = 0
      loop {
        break when opts.count >= 0 and (if opts.count_bytes { read >= opts.count } else { blocks >= opts.count })
        let want = if opts.count_bytes and opts.count >= 0 and opts.count - read < opts.ibs { opts.count - read } else { opts.ibs }
        guard let first = io.stdin_read(want) else { |failure| gnu.error(f"error reading 'standard input': {gnu.strerror(failure)}"); exit 1 }
        break when first.is_empty()
        var pieces = [first]
        var length = first.len()
        if opts.fullblock {
          while length < want {
            let part = io.stdin_read(want - length)?
            break when part.is_empty()
            pieces += [part]
            length += part.len()
          }
        }
        records += [bytes.concat(pieces)]
        read += length
        blocks += 1
      }
    }
  }
  let complete = [record for record in records if record.len() == opts.ibs].len()
  let converted = transform(records, opts)?
  if opts.output == "-" { gnu.write_bytes(converted.data); io.flush_stdout()? } else {
    let dest = fp"{opts.output}"
    if "nocreat" in opts.conv and ! dest.exists() { gnu.error(f"failed to open {gnu.quote(opts.output)}: No such file or directory"); exit 1 }
    if ! ("notrunc" in opts.conv) {
      if ! dest.exists() and ! ("nocreat" in opts.conv) { dest.write(b"")? }
      if dest.metadata()?.mode / 4096 % 16 == 8 { dest.truncate(seek)? }
    }
    let kind = dest.metadata()?.mode / 4096 % 16
    if kind in [1, 8] {
      let fd = unix.open_fd(dest, write: true, nonblock: false)?
      if kind == 8 { let _ = unix.seek_fd(fd, seek)? }
      write_fd_all(fd, converted.data)?
      unix.close_fd(fd)?
    } else {
      guard let _ = bytes.write_at(dest, seek, converted.data, create: ! ("nocreat" in opts.conv)) else { |failure|
        gnu.error(f"error writing {gnu.quote(opts.output)}: {gnu.strerror(failure)}")
        exit 1
      }
    }
  }
  report(opts, converted.data.len(), complete, records.len() - complete, converted.truncated, started)
}

proc report(opts: Options, size: Int, complete: Int, partial: Int, truncated: Int, started: Int) [time] {
  if opts.status != "none" {
    eprint f"{complete}+{partial} records in"
    eprint f"{if opts.bs > 0 and truncated == 0 { complete } else { size / opts.obs }}+{if opts.bs > 0 and truncated == 0 { partial } else if size % opts.obs == 0 { 0 } else { 1 }} records out"
    if truncated > 0 { eprint f"{truncated} truncated {if truncated == 1 { "record" } else { "records" }}" }
    if opts.status != "noxfer" {
      let elapsed = time.now() - started
      let millis = if elapsed < 1 { 1 } else { elapsed }
      let si = if size >= 1000 { human_size(size, 1000, false) } else { "" }
      let iec = if size >= 1024 { human_size(size, 1024, true) } else { "" }
      let units = if si == "" { "" } else if iec == "" { f" ({si})" } else { f" ({si}, {iec})" }
      eprint f"{size} bytes{units} copied, {millis / 1000}.{millis % 1000:03} s, {if size == 0 { "0.0 B" } else { human_size(size * 1000 / millis, 1000, false) }}/s"
    }
  }
}
