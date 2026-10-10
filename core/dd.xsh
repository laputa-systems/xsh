#!/bin/xsh
use lib.gnu
use lib.bytes_enc_dd as charset
use lib.textio_a1 as tio

type Options = {input: Path?, output: Path?, ibs: Int, obs: Int, cbs: Int, count: Int, skip: Int, seek: Int, count_bytes: Bool, skip_bytes: Bool, seek_bytes: Bool, conv: List[Str], status: Str, fullblock: Bool, input_directory: Bool, bs: Int}
type BlockOutput = {data: Bytes, padding: Int, pad_byte: Int}
type Converted = {data: Bytes, blocks: List[BlockOutput], truncated: Int}

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

proc rich_diagnostics_enabled() [env, process] -> Bool {
  let requested = env.get_or("UUTILS_DIAG", "") ?? ""
  return true when requested == "always"
  return false when requested == "never"
  unix.isatty(2)
}

## Reconstruct the operand location because dd parses each KEY=VALUE argument directly.
proc render_operand_error(argv: List[Bytes], arg_index: Int, message: Str, span_start: Int, span_length: Int, label: Str?, help: Str, usage: Bool, status: Int) [env, process, error] {
  if ! rich_diagnostics_enabled() {
    gnu.error(message)
    if usage { gnu.try_help() }
    exit status
  }

  gnu.error(message)
  let program = gnu.prog()
  let command = [program].extend([argument.utf8() ?? gnu.quote_bytes(argument) for argument in argv]).join(" ")
  var column = program.byte_len() + 1
  for index in range(arg_index) { column += argv[index].len() + 1 }
  column += span_start
  let marker = if label == null { ["─" for _ in range(span_length)].join("") } else { "─┬" }
  let spacing = [" " for _ in range(column)].join("")
  eprint f"   ╭─[ {program}:1:{column + 1} ]"
  eprint "   │"
  eprint f" 1 │ {command}"
  eprint f"   │ {spacing}{marker}"
  if let note = label {
    eprint f"   │ {spacing} ╰─ {note}"
  } else {
    eprint "   │"
  }
  if help != "" { eprint f"   │ Help: {help}" }
  eprint "───╯"
  if usage { gnu.try_help() }
  exit status
}

pure assignment_separator(arg: Bytes) -> Int? {
  for at in range(arg.len()) { if arg.byte_at(at) == 61 { return at } }
  null
}

proc parse(argv: List[Bytes]) [process, env, io, error] -> Options {
  var opts: Options = {input: null, output: null, ibs: 512, obs: 512, cbs: 0, count: -1, skip: 0, seek: 0, count_bytes: false, skip_bytes: false, seek_bytes: false, conv: [], status: "default", fullblock: false, input_directory: false, bs: 0}
  for arg_index in range(argv.len()) {
    let raw_arg = argv[arg_index]
    let separator = assignment_separator(raw_arg)
    let byte_key = if let at = separator { raw_arg[0..at].utf8() ?? "" } else { "" }
    if let at = separator {
      if byte_key in ["if", "of"] {
        let value = raw_arg[at + 1..]
        let operand_path: Path? = if value == b"-" { null } else { Path.parse_bytes(value)? }
        opts = if byte_key == "if" { {...opts, input: operand_path} } else { {...opts, output: operand_path} }
        continue
      }
    }
    guard let arg = raw_arg.utf8() else {
      render_operand_error(argv, arg_index, f"invalid UTF-8 in operand: {gnu.quote_bytes(raw_arg)}", 0, raw_arg.len(), null, "file names may contain arbitrary bytes", false, 1)
      exit 1
    }
    let parts = arg.split("=")
    if parts.len() < 2 { render_operand_error(argv, arg_index, f"unrecognized operand {gnu.quote(arg)}", 0, arg.byte_len(), null, "an operand is KEY=VALUE, as in if=file bs=4k count=10", true, 1) }
    let key = parts[0]
    let text = arg.byte_slice(key.byte_len() + 1)
    if key in ["ibs", "obs", "bs", "cbs", "count", "skip", "seek", "iseek", "oseek"] {
      let factors = text.split("x")
      for index in range(factors.len() - 1) {
        if factors[index] == "0" { gnu.error("warning: '0x' is a zero multiplier; use '00x' if that is intended") }
      }
      let number_start = key.byte_len() + 1
      let number_help = "a number may be followed by a multiplier: c, w, b, then K, M, G and so on for 1024, kB, MB, GB for 1000"
      guard let n = amount(text) else { render_operand_error(argv, arg_index, f"invalid number: {gnu.quote(text)}", number_start, text.byte_len(), null, number_help, false, 1); exit 1 }
      if n == 9223372036854775807 { render_operand_error(argv, arg_index, f"invalid number: {gnu.quote(text)}: Value too large for defined data type", number_start, text.byte_len(), null, number_help, false, 1) }
      if n == 0 and key in ["ibs", "obs", "bs", "cbs"] { render_operand_error(argv, arg_index, f"invalid number: {gnu.quote(text)}", number_start, text.byte_len(), null, number_help, false, 1) }
      let byte_count = [factor for factor in text.split("x") if rx"^[0-9]+B$".matches(factor)].len() > 0
      if key == "bs" { opts = {...opts, ibs: n, obs: n, bs: n} } else if key == "ibs" { opts = {...opts, ibs: n} } else if key == "obs" { opts = {...opts, obs: n} } else if key == "cbs" { opts = {...opts, cbs: n} } else if key == "count" { opts = {...opts, count: n, count_bytes: byte_count} } else if key in ["skip", "iseek"] { opts = {...opts, skip: n, skip_bytes: byte_count} } else { opts = {...opts, seek: n, seek_bytes: byte_count} }
    } else if key == "status" {
      if ! (text in ["none", "noxfer", "progress"]) { gnu.usage_error(f"invalid status level: {gnu.quote(text)}") }
      if text == "progress" { gnu.usage_error("unsupported status level progress: interruptible transfer reporting required") }
      opts = {...opts, status: text}
    } else if key in ["conv", "iflag", "oflag"] {
      for flag in text.split(",") {
        if key == "conv" {
          if flag in ["lcase", "ucase", "swab", "sync", "block", "unblock", "notrunc", "nocreat", "ascii", "ebcdic", "ibm", "sparse"] { opts = {...opts, conv: opts.conv.extend([flag])} } else if flag in ["excl", "noerror", "fdatasync", "fsync"] { gnu.usage_error(f"unsupported conversion {gnu.quote(flag)}: native descriptor support required") } else {
            var start = key.byte_len() + 1
            for list_flag in text.split(",") {
              if list_flag == flag { break }
              start += list_flag.byte_len() + 1
            }
            render_operand_error(argv, arg_index, f"invalid conversion: {gnu.quote(flag)}", start, flag.byte_len(), "not a known conversion", "conv= is one of ascii, ebcdic, ibm, lcase, ucase, block, unblock, swab, sync, noerror, sparse, excl, nocreat, notrunc, fdatasync or fsync", true, 1)
          }
        } else {
          if key in ["iflag", "oflag"] and flag == "count_bytes" { opts = {...opts, count_bytes: true} } else if key == "iflag" and flag == "skip_bytes" { opts = {...opts, skip_bytes: true} } else if key == "iflag" and flag == "fullblock" { opts = {...opts, fullblock: true} } else if key == "iflag" and flag == "directory" { opts = {...opts, input_directory: true} } else if key == "oflag" and flag == "seek_bytes" { opts = {...opts, seek_bytes: true} } else if flag in ["direct", "directory", "dsync", "sync", "append", "nonblock", "noatime", "nocache", "nofollow", "nolinks", "cio", "text", "binary", "excl"] { gnu.usage_error(f"unsupported {key} {gnu.quote(flag)}: native descriptor support required") } else {
            var start = key.byte_len() + 1
            for list_flag in text.split(",") {
              if list_flag == flag { break }
              start += list_flag.byte_len() + 1
            }
            let direction = if key == "iflag" { "input" } else { "output" }
            let label = if key == "iflag" { "not a known input flag" } else { "not a known output flag" }
            let help = if key == "iflag" { "iflag= is one of direct, directory, dsync, sync, nocache, nonblock, noatime, noctty, nofollow, fullblock, count_bytes or skip_bytes" } else { "oflag= is one of direct, directory, dsync, sync, nocache, nonblock, noatime, noctty, nofollow, append or seek_bytes" }
            render_operand_error(argv, arg_index, f"invalid {direction} flag: {gnu.quote(flag)}", start, flag.byte_len(), label, help, true, 1)
          }
        }
      }
    } else { render_operand_error(argv, arg_index, f"unrecognized operand {gnu.quote(arg)}", 0, key.byte_len(), null, "an operand is KEY=VALUE, as in if=file bs=4k count=10", true, 1) }
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

# Keep dd's path operands byte-native while matching textio's source classification.
proc open_dd_source(source_path: Path) [fs, error] -> Result[tio.Source] {
  let target = source_path.resolve()?
  let entry = target.metadata()?
  let kind = entry.mode / 4096 % 16
  let mode = if kind == 8 and entry.size > 0 { "file" } else if kind == 2 or kind == 6 { "device" } else { "whole" }
  Ok({name: target.display(), path: target, mode: mode, kind: kind, size: entry.size})
}

proc quoted_path(file_path: Path) [env] -> Str {
  gnu.quote_bytes(file_path.bytes())
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
    var truncated = 0
    var block_outputs: List[BlockOutput] = []
    var start = 0
    for at in range(mapped.len() + 1) {
      if at == mapped.len() or mapped.byte_at(at) == 10 {
        if at == mapped.len() and start == at { break }
        let length = at - start
        if length > opts.cbs { truncated += 1 }
        let width = if length < opts.cbs { length } else { opts.cbs }
        let data = encode_charset(mapped.slice(start, length: width), opts)?
        let pad_byte = if "ebcdic" in opts.conv or "ibm" in opts.conv { charset.character(32, if "ibm" in opts.conv { "ibm" } else { "ebcdic" }) } else { 32 }
        block_outputs += [{data: data, padding: opts.cbs - width, pad_byte: pad_byte}]
        start = at + 1
      }
    }
    return Ok({data: b"", blocks: block_outputs, truncated: truncated})
  }
  if "unblock" in opts.conv {
    var out: List[Bytes] = []
    for block in mapped.chunks(opts.cbs) {
      var end = block.len()
      while end > 0 and block.byte_at(end - 1) == 32 { end -= 1 }
      out += [block[0..end], b"\n"]
    }
    return Ok({data: encode_charset(bytes.concat(out), opts)?, blocks: [], truncated: 0})
  }
  Ok({data: encode_charset(mapped, opts)?, blocks: [], truncated: 0})
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

type WriteOutcome = {written: Int, failure: Str?}
type Copied = {complete: Int, partial: Int, written: Int, failure: Str?}

proc write_fd_all(fd: Int, data: Bytes) [process] -> WriteOutcome {
  var offset = 0
  while offset < data.len() {
    guard let written = unix.write_fd(fd, data[offset..]) else { |failure|
      return {written: offset, failure: gnu.strerror(failure)}
    }
    if written == 0 { return {written: offset, failure: "descriptor write made no progress"} }
    offset += written
  }
  {written: offset, failure: null}
}

pure all_zero(data: Bytes) -> Bool {
  for index in range(data.len()) {
    if data.byte_at(index) != 0 { return false }
  }
  true
}

proc write_output_chunk(fd: Int, data: Bytes, offset: Int, sparse: Bool) [error, process] -> Result[WriteOutcome] {
  if sparse and all_zero(data) { return Ok({written: data.len(), failure: null}) }
  if sparse { let _ = unix.seek_fd(fd, offset)? }
  Ok(write_fd_all(fd, data))
}

proc write_path_chunk(dest: Path, offset: Int, data: Bytes, create: Bool) [fs, process] -> WriteOutcome {
  guard let written = bytes.write_at(dest, offset, data, create:) else { |failure|
    return {written: 0, failure: gnu.strerror(failure)}
  }
  if written < data.len() { return {written: written, failure: "short file write"} }
  {written: written, failure: null}
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

proc seek_output_fifo(dest: Path, offset: Int, block_size: Int) [fs, process, error] -> Result[Unit] {
  let fd = unix.open_fd(dest, nonblock: false)?
  var discarded = 0
  while discarded < offset {
    let width = if offset - discarded < block_size { offset - discarded } else { block_size }
    let block = unix.read_fd(fd, width)?
    break when block.is_empty()
    discarded += block.len()
  }
  unix.close_fd(fd)?
  Ok()
}

proc plain_copy(opts: Options, skip: Int, seek: Int, limit: Int) [fs, error, io, process, env] -> Result[Copied] {
  let dest = if let output = opts.output { output } else { p"" }
  var output_fd: Int? = null
  var output_kind = 0
  if opts.output != null {
    if "nocreat" in opts.conv and ! dest.exists() { fail "No such file or directory" }
    if ! dest.exists() and ! ("nocreat" in opts.conv) { dest.write(b"")? }
    output_kind = dest.metadata()?.mode / 4096 % 16
    if output_kind == 1 and seek > 0 { seek_output_fifo(dest, seek, opts.obs)? }
    if output_kind in [1, 2, 6, 8] and ! (output_kind == 1 and opts.count == 0) {
      output_fd = unix.open_fd(dest, write: true, nonblock: false)?
    }
    if output_kind == 8 {
      if ! ("notrunc" in opts.conv) { dest.truncate(seek)? }
      if let fd = output_fd { let _ = unix.seek_fd(fd, seek)? }
    }
  }
  var source: tio.Source? = null
  if let input = opts.input { source = open_dd_source(input)? }
  var input_fd: Int? = null
  if let file = source {
    if file.kind in [1, 2, 6] { input_fd = unix.open_fd(file.path, nonblock: false)? }
    if file.mode == "file" and opts.output != null and output_kind == 8 and ! ("sparse" in opts.conv) {
      let size = file.path.metadata()?.size
      if skip > size {
        let input = opts.input ?? p""
        gnu.error(f"{quoted_path(input)}: cannot skip to specified offset")
      }
      let available = if size > skip { size - skip } else { 0 }
      let length = if available < limit { available } else { limit }
      if let fd = output_fd { unix.close_fd(fd)?; output_fd = null }
      let copied = bytes.copy_file(file.path, dest.resolve()?, source_offset: skip, dest_offset: seek, length: length, create: ! ("nocreat" in opts.conv))?
      return Ok({complete: copied.bytes / opts.ibs, partial: if copied.bytes % opts.ibs == 0 { 0 } else { 1 }, written: copied.bytes, failure: null})
    }
  }
  let large_input = if let file = source { opts.ibs > 67108864 and file.kind != 1 } else { false }
  if let fd = input_fd {
    var discarded = 0
    while discarded < skip {
      let requested = if skip - discarded < opts.ibs { skip - discarded } else { opts.ibs }
      let want = if requested > 65536 { 65536 } else { requested }
      let block = unix.read_fd(fd, want)?
      break when block.is_empty()
      discarded += block.len()
    }
  } else if source == null {
    var discarded = 0
    while discarded < skip {
      let requested = if skip - discarded < opts.ibs { skip - discarded } else { opts.ibs }
      let want = if requested > 65536 { 65536 } else { requested }
      let block = io.stdin_read(want)?
      break when block.is_empty()
      discarded += block.len()
    }
    if discarded < skip { gnu.error("'standard input': cannot skip to specified offset") }
  }
  var total = 0
  var complete = 0
  var partial = 0
  var written = 0
  var failure: Str? = null
  var output_chunks: List[Bytes] = []
  var output_length = 0
  var output_position = 0
  let sparse_output = output_kind == 8 and "sparse" in opts.conv and ! ("notrunc" in opts.conv)
  while failure == null {
    let finished_count = if large_input { total / opts.ibs >= opts.count } else { complete + partial >= opts.count }
    break when total >= limit or (opts.count >= 0 and ! opts.count_bytes and finished_count)
    let requested = if limit - total < opts.ibs { limit - total } else { opts.ibs }
    let want = if large_input and requested > 65536 { 65536 } else { requested }
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
    let offset = total
    total += block.len()
    if large_input {
      complete = total / opts.ibs
      partial = if total % opts.ibs == 0 { 0 } else { 1 }
    } else if block.len() == opts.ibs { complete += 1 } else { partial += 1 }
    if opts.output == null {
      gnu.write_bytes(block)
      io.flush_stdout()?
      written += block.len()
    } else if let fd = output_fd {
      var rest = block
      while ! rest.is_empty() and failure == null {
        let record_remaining = opts.obs - output_position % opts.obs
        let chunk_limit = if record_remaining < 65536 { record_remaining } else { 65536 }
        let needed = chunk_limit - output_length
        let width = if rest.len() < needed { rest.len() } else { needed }
        output_chunks += [rest.slice(0, length: width)]
        output_length += width
        output_position += width
        rest = rest[width..]
        if output_length == chunk_limit {
          let output_chunk = bytes.concat(output_chunks)
          let outcome = write_output_chunk(fd, output_chunk, seek + output_position - output_length, sparse_output)?
          output_chunks = []
          output_length = 0
          written += outcome.written
          failure = outcome.failure
        }
      }
    } else {
      let outcome = write_path_chunk(dest, seek + offset, block, ! ("nocreat" in opts.conv))
      written += outcome.written
      failure = outcome.failure
    }
  }
  if let fd = output_fd {
    if output_length > 0 and failure == null {
      let output_chunk = bytes.concat(output_chunks)
      let outcome = write_output_chunk(fd, output_chunk, seek + output_position - output_length, sparse_output)?
        written += outcome.written
        failure = outcome.failure
    }
  }
  if sparse_output and failure == null { dest.truncate(seek + output_position)? }
  if let fd = input_fd { unix.close_fd(fd)? }
  if let fd = output_fd { unix.close_fd(fd)? }
  Ok({complete: complete, partial: partial, written: written, failure: failure})
}

## iflag=directory has no XSH open flag. Checking the file type before any read
## gives the outcome of GNU's O_DIRECTORY open. Standard input is inspected
## through /dev/stdin because the descriptor is already open.
proc require_directory_input(input: Path?) [fs, process, env, error, io] {
  let target = input ?? p"/dev/stdin"
  guard let meta = fs.stat(target, follow_symlinks: true) else { |failure|
    if let input_path = input {
      gnu.error(f"failed to open {quoted_path(input_path)}: {gnu.strerror(failure)}")
    } else {
      gnu.error(f"setting flags for 'standard input': {gnu.strerror(failure)}")
    }
    exit 1
  }
  return when meta.kind == "dir"
  if let input_path = input {
    gnu.error(f"failed to open {quoted_path(input_path)}: Not a directory")
  } else {
    gnu.error("setting flags for 'standard input': Not a directory")
  }
  exit 1
}

proc main(...argv: List[Bytes]) [fs, process, env, error, io, time] {
  if b"--help" in argv { gnu.help("Usage: dd [OPERAND]...\nCopy a file, converting and formatting according to the operands.\n\nOperands:\n  if=FILE of=FILE bs=BYTES ibs=BYTES obs=BYTES cbs=BYTES\n  count=N skip=N seek=N status=none|noxfer|progress\n\nConversion options:\n  conv=ascii,ebcdic,ibm,block,unblock,lcase,ucase,swab,sync,sparse,notrunc,nocreat\n  iflag=count_bytes,skip_bytes,fullblock oflag=seek_bytes\nNative descriptor flags are not supported."); return }
  if b"--version" in argv { gnu.version("dd"); return }
  let started = time.now()
  let opts = parse(argv)
  if "sync" in opts.conv and opts.ibs > 67108864 { gnu.error("memory exhausted"); exit 1 }
  if opts.obs > 67108864 and opts.bs == 0 { gnu.error("memory exhausted"); exit 1 }
  if opts.skip > 0 and ! opts.skip_bytes and opts.skip > 9223372036854775807 / opts.ibs { gnu.error("Value too large for defined data type"); exit 1 }
  if opts.seek > 0 and ! opts.seek_bytes and opts.seek > 9223372036854775807 / opts.obs { gnu.error("Value too large for defined data type"); exit 1 }
  let skip = if opts.skip_bytes { opts.skip } else { opts.skip * opts.ibs }
  let seek = if opts.seek_bytes { opts.seek } else { opts.seek * opts.obs }
  if opts.output == null { prepare_stdout_seek(seek) }
  if opts.count > 0 and ! opts.count_bytes and opts.count > 9223372036854775807 / opts.ibs { gnu.error("count is too large"); exit 1 }
  let limit = if opts.count < 0 { 9223372036854775807 } else if opts.count_bytes { opts.count } else { opts.count * opts.ibs }
  if let input = opts.input {
    if input.bytes().is_empty() { gnu.error("failed to open '': No such file or directory"); exit 1 }
    if let Err(failure) = open_dd_source(input) { gnu.error(f"failed to open {quoted_path(input)}: {gnu.strerror(failure)}"); exit 1 }
  }
  if let output = opts.output {
    if output.bytes().is_empty() { gnu.error("failed to open '': No such file or directory"); exit 1 }
    if "nocreat" in opts.conv and ! output.exists() { gnu.error(f"failed to open {quoted_path(output)}: No such file or directory"); exit 1 }
  }
  if opts.input_directory { require_directory_input(opts.input) }
  let plain = [flag for flag in opts.conv if ! (flag in ["notrunc", "nocreat", "sparse"])].is_empty()
  if plain {
    guard let copied = plain_copy(opts, skip, seek, limit) else { |failure| gnu.error(gnu.strerror(failure)); exit 1 }
    report(opts, copied.written, copied.complete, copied.partial, 0, started, output_failed: copied.failure != null)
    if let failure = copied.failure { gnu.error(failure); exit 1 }
    return
  }
  var records: List[Bytes] = []
  var fifo_fd: Int? = null
  if let input = opts.input {
    guard let source = open_dd_source(input) else { |failure| gnu.error(f"failed to open {quoted_path(input)}: {gnu.strerror(failure)}"); exit 1 }
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
    if let input = opts.input {
      guard let source = open_dd_source(input) else { |failure| gnu.error(f"failed to open {quoted_path(input)}: {gnu.strerror(failure)}"); exit 1 }
      var chunks: List[Bytes] = []
      var at = skip
      var remaining = limit
      loop {
        let width = if remaining < opts.ibs { remaining } else { opts.ibs }
        break when width == 0
        guard let block = tio.read_chunk(source, at, width) else { |failure| gnu.error(f"error reading {quoted_path(input)}: {gnu.strerror(failure)}"); exit 1 }
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
  var written = 0
  var write_failure: Str? = null
  if opts.output == null {
    if converted.blocks.is_empty() {
      gnu.write_bytes(converted.data)
      written = converted.data.len()
    } else {
      for record in converted.blocks {
        gnu.write_bytes(record.data)
        written += record.data.len()
        var padding = record.padding
        while padding > 0 {
          let width = if padding < 8192 { padding } else { 8192 }
          let bytes_out = bytes.from_ints([record.pad_byte for _ in range(width)])?
          gnu.write_bytes(bytes_out)
          written += width
          padding -= width
        }
      }
    }
    io.flush_stdout()?
  } else {
    let dest = opts.output ?? p""
    if "nocreat" in opts.conv and ! dest.exists() { gnu.error(f"failed to open {quoted_path(dest)}: No such file or directory"); exit 1 }
    if ! ("notrunc" in opts.conv) {
      if ! dest.exists() and ! ("nocreat" in opts.conv) { dest.write(b"")? }
      if dest.metadata()?.mode / 4096 % 16 == 8 { dest.truncate(seek)? }
    }
    let kind = dest.metadata()?.mode / 4096 % 16
    if kind == 1 and seek > 0 { seek_output_fifo(dest, seek, opts.obs)? }
    if kind in [1, 2, 6, 8] {
      if kind != 1 or opts.count != 0 {
        let fd = unix.open_fd(dest, write: true, nonblock: false)?
        if kind == 8 { let _ = unix.seek_fd(fd, seek)? }
        if converted.blocks.is_empty() {
          let outcome = write_fd_all(fd, converted.data)
          written += outcome.written
          write_failure = outcome.failure
        } else {
          for record in converted.blocks {
            break when write_failure != null
            let outcome = write_fd_all(fd, record.data)
            written += outcome.written
            write_failure = outcome.failure
            var padding = record.padding
            while padding > 0 and write_failure == null {
              let width = if padding < 8192 { padding } else { 8192 }
              let bytes_out = bytes.from_ints([record.pad_byte for _ in range(width)])?
              let pad_outcome = write_fd_all(fd, bytes_out)
              written += pad_outcome.written
              write_failure = pad_outcome.failure
              padding -= pad_outcome.written
            }
          }
        }
        unix.close_fd(fd)?
      }
    } else {
      if converted.blocks.is_empty() {
        let outcome = write_path_chunk(dest, seek, converted.data, ! ("nocreat" in opts.conv))
        written += outcome.written
        write_failure = outcome.failure
      } else {
        var offset = seek
        for record in converted.blocks {
          break when write_failure != null
          let outcome = write_path_chunk(dest, offset, record.data, ! ("nocreat" in opts.conv))
          written += outcome.written
          offset += outcome.written
          write_failure = outcome.failure
          var padding = record.padding
          while padding > 0 and write_failure == null {
            let width = if padding < 65536 { padding } else { 65536 }
            let bytes_out = bytes.from_ints([record.pad_byte for _ in range(width)])?
            let pad_outcome = write_path_chunk(dest, offset, bytes_out, ! ("nocreat" in opts.conv))
            written += pad_outcome.written
            offset += pad_outcome.written
            write_failure = pad_outcome.failure
            padding -= pad_outcome.written
          }
        }
      }
    }
  }
  report(opts, written, complete, records.len() - complete, converted.truncated, started, output_failed: write_failure != null)
  if let failure = write_failure { gnu.error(failure); exit 1 }
}

proc report(opts: Options, size: Int, complete: Int, partial: Int, truncated: Int, started: Int, output_failed = false) [time] {
  if opts.status != "none" {
    eprint f"{complete}+{partial} records in"
    let output_complete = if opts.bs > 0 and truncated == 0 and ! output_failed { complete } else { size / opts.obs }
    let output_partial = if opts.bs > 0 and truncated == 0 and ! output_failed { partial } else if size % opts.obs == 0 { 0 } else { 1 }
    eprint f"{output_complete}+{output_partial} records out"
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
