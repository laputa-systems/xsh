#!/bin/xsh
use lib.gnu

const USAGE = """Usage: dd [OPERAND]...
Copy a file, converting and formatting according to the operands.

  if=FILE     read from FILE instead of standard input
  of=FILE     write to FILE instead of standard output
  bs=BYTES    set both input and output block sizes
  ibs=BYTES   set the input block size
  obs=BYTES   set the output block size
  count=N     copy only N input blocks
  skip=N      skip N input blocks
  seek=N      skip N output blocks
  conv=LIST   apply supported conversions
  status=none suppress transfer statistics
      --help  display this help and exit
      --version output version information and exit
"""

type Params = {
  input: Str?, output: Str?, ibs: Int, obs: Int, cbs: Int, count: Int?, skip: Int, seek: Int,
  count_bytes: Bool, skip_bytes: Bool, seek_bytes: Bool, notrunc: Bool, status: Str,
  conversions: List[Str], input_flags: List[Str], output_flags: List[Str]
}

const MAX_DD_SIZE = 9223372036854775806

pure parse_size(text: Str) -> Int? {
  if text == "" { return null }
  let product = text.find("x")
  if product != null {
    let left = parse_size(text.byte_slice(0, product ?? 0))
    let right = parse_size(text.byte_slice((product ?? 0) + 1))
    if left == null or right == null or ((right ?? 0) > 0 and (left ?? 0) > MAX_DD_SIZE / (right ?? 1)) { return null }
    return (left ?? 0) * (right ?? 1)
  }
  let mult = size_multiplier(text)
  let suffix = size_suffix_length(text)
  let digits = if suffix == 0 { text } else { text.byte_slice(0, text.byte_len() - suffix) }
  let parsed = digits.parse_int()
  if let Ok(value) = parsed {
    if value < 0 or value > MAX_DD_SIZE / mult { return null }
    return value * mult
  }
  null
}

pure size_multiplier(text: Str) -> Int {
  if text.ends_with("KiB") { return 1024 }
  if text.ends_with("MiB") { return 1048576 }
  if text.ends_with("GiB") { return 1073741824 }
  if text.ends_with("KB") { return 1000 }
  if text.ends_with("MB") { return 1000000 }
  if text.ends_with("GB") { return 1000000000 }
  if text.ends_with("k") or text.ends_with("K") { return 1024 }
  if text.ends_with("m") or text.ends_with("M") { return 1048576 }
  if text.ends_with("g") or text.ends_with("G") { return 1073741824 }
  if text.ends_with("b") { return 512 }
  1
}

pure size_suffix_length(text: Str) -> Int {
  if text.ends_with("KiB") or text.ends_with("MiB") or text.ends_with("GiB") { return 3 }
  if text.ends_with("KB") or text.ends_with("MB") or text.ends_with("GB") { return 2 }
  if text.ends_with("B") { return 1 }
  if text.ends_with("k") or text.ends_with("K") or text.ends_with("m") or text.ends_with("M") or text.ends_with("g") or text.ends_with("G") or text.ends_with("b") { return 1 }
  0
}

pure decimal_digits(text: Str) -> Bool {
  if text == "" { return false }
  for index in range(text.byte_len()) {
    if text.byte_slice(index, 1) not in ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"] { return false }
  }
  true
}

pure size_overflows(text: Str) -> Bool {
  let product = text.find("x")
  if product != null {
    let left_text = text.byte_slice(0, product ?? 0)
    let right_text = text.byte_slice((product ?? 0) + 1)
    if size_overflows(left_text) or size_overflows(right_text) { return true }
    let left = parse_size(left_text)
    let right = parse_size(right_text)
    return left != null and right != null and (right ?? 0) > 0 and (left ?? 0) > MAX_DD_SIZE / (right ?? 1)
  }
  let suffix = size_suffix_length(text)
  let digits = if suffix == 0 { text } else { text.byte_slice(0, text.byte_len() - suffix) }
  if ! decimal_digits(digits) { return false }
  let value = digits.parse_int()
  if let Ok(number) = value { number > MAX_DD_SIZE / size_multiplier(text) } else { true }
}

pure zero_multiplier_count(text: Str) -> Int {
  var count = 0
  var start = 0
  for index in range(text.byte_len()) {
    if text.byte_slice(index, 1) == "x" {
      if text.byte_slice(start, index - start) == "0" { count += 1 }
      start = index + 1
    }
  }
  count
}

pure last_value(args: List[Str], keys: List[Str], fallback: Str) -> Str {
  var value = fallback
  for arg in args {
    let equal = arg.find("=")
    if equal != null and arg.byte_slice(0, equal ?? 0) in keys { value = arg.byte_slice((equal ?? 0) + 1) }
  }
  value
}

proc invalid_number(value: Str, too_large = false) [process, env] -> Unit {
  let detail = if too_large { ": Value too large for defined data type" } else { "" }
  gnu.error(f"invalid number: {gnu.quote_value(value)}{detail}")
  exit 1
}

pure fields(args: List[Str]) -> Params {
  var input: Str? = null
  var output: Str? = null
  var ibs = 512
  var obs = 512
  var cbs = 0
  var block: Int? = null
  var count: Int? = null
  var skip = 0
  var seek = 0
  var count_bytes = false
  var skip_bytes = false
  var seek_bytes = false
  var notrunc = false
  var status = "default"
  var conversions: List[Str] = []
  var input_flags: List[Str] = []
  var output_flags: List[Str] = []
  for arg in args {
    let equal = arg.find("=")
    if equal == null { continue }
    let key = arg.byte_slice(0, equal ?? 0)
    let value = arg.byte_slice((equal ?? 0) + 1)
    if key == "if" { input = value }
    if key == "of" { output = value }
    if key == "bs" { block = parse_size(value) ?? -1 }
    if key == "ibs" { ibs = parse_size(value) ?? -1 }
    if key == "obs" { obs = parse_size(value) ?? -1 }
    if key == "cbs" { cbs = parse_size(value) ?? -1 }
    if key == "count" { count = parse_size(value) ?? -1; count_bytes = count_bytes or value.find("B") != null }
    if key == "skip" { skip = parse_size(value) ?? -1; skip_bytes = skip_bytes or value.find("B") != null }
    if key == "seek" { seek = parse_size(value) ?? -1; seek_bytes = seek_bytes or value.find("B") != null }
    if key == "iseek" { skip = parse_size(value) ?? -1; skip_bytes = skip_bytes or value.find("B") != null }
    if key == "oseek" { seek = parse_size(value) ?? -1; seek_bytes = seek_bytes or value.find("B") != null }
    if key == "conv" { conversions = value.split(",") }
    if key == "iflag" { input_flags = value.split(","); count_bytes = count_bytes or "count_bytes" in input_flags; skip_bytes = skip_bytes or "skip_bytes" in input_flags }
    if key == "oflag" { output_flags = value.split(","); seek_bytes = "seek_bytes" in output_flags; count_bytes = count_bytes or "count_bytes" in output_flags }
    if key == "status" { status = value }
  }
  if block != null { ibs = block ?? 512; obs = block ?? 512 }
  {input: input, output: output, ibs: ibs, obs: obs, cbs: cbs, count: count, skip: skip, seek: seek, count_bytes: count_bytes, skip_bytes: skip_bytes, seek_bytes: seek_bytes, notrunc: notrunc or "notrunc" in conversions, status: status, conversions: conversions, input_flags: input_flags, output_flags: output_flags}
}

proc write_output(dest: Path, data: Bytes, offset: Int, notrunc: Bool) [fs, process, env, error] -> Bool {
  if notrunc or offset > 0 {
    match bytes.write_at(dest, offset, data, create: true) {
      Ok(_) => true,
      Err(failure) => { gnu.error(f"error writing {gnu.quote(dest.display())}: {gnu.strerror(failure)}"); false },
    }
  } else {
    match dest.write(data) {
      Ok(_) => true,
      Err(failure) => { gnu.error(f"error writing {gnu.quote(dest.display())}: {gnu.strerror(failure)}"); false },
    }
  }
}

pure ascii_case(data: Bytes, upper: Bool) -> Bytes {
  var values: List[Int] = []
  for index in range(data.len()) {
    let byte = data.byte_at(index) ?? 0
    values += [if upper and byte >= 97 and byte <= 122 { byte - 32 } else if ! upper and byte >= 65 and byte <= 90 { byte + 32 } else { byte }]
  }
  bytes.from_ints(values) ?? b""
}

pure swab(data: Bytes) -> Bytes {
  var values: List[Int] = []
  var at = 0
  while at + 1 < data.len() { values += [data.byte_at(at + 1) ?? 0, data.byte_at(at) ?? 0]; at += 2 }
  if at < data.len() { values += [data.byte_at(at) ?? 0] }
  bytes.from_ints(values) ?? b""
}

pure record_block(data: Bytes, width: Int) -> Bytes {
  var output: List[Int] = []
  var record: List[Int] = []
  for index in range(data.len()) {
    let byte = data.byte_at(index) ?? 0
    if byte == 10 {
      let kept = if record.len() < width { record.len() } else { width }
      for i in range(kept) { output += [record[i]] }
      for _ in range(width - kept) { output += [32] }
      record = []
    } else { record += [byte] }
  }
  if record.len() > 0 {
    let kept = if record.len() < width { record.len() } else { width }
    for i in range(kept) { output += [record[i]] }
    for _ in range(width - kept) { output += [32] }
  }
  bytes.from_ints(output) ?? b""
}

pure record_unblock(data: Bytes, width: Int) -> Bytes {
  var output: List[Int] = []
  var start = 0
  while start < data.len() {
    let end = if start + width < data.len() { start + width } else { data.len() }
    var stop = end
    while stop > start and data.byte_at(stop - 1) == 32 { stop -= 1 }
    for index in range(start, stop) { output += [data.byte_at(index) ?? 0] }
    output += [10]
    start += width
  }
  bytes.from_ints(output) ?? b""
}

pure all_zero(data: Bytes) -> Bool {
  for index in range(data.len()) { if (data.byte_at(index) ?? 0) != 0 { return false } }
  true
}

pure truncated_records(data: Bytes, width: Int) -> Int {
  var count = 0
  var length = 0
  for index in range(data.len()) {
    if (data.byte_at(index) ?? 0) == 10 {
      if length > width { count += 1 }
      length = 0
    } else {
      length += 1
    }
  }
  if length > width { count += 1 }
  count
}

proc write_sparse_output(dest: Path, data: Bytes, offset: Int, notrunc: Bool, block_size: Int) [fs, process, env, error] -> Bool {
  if offset != 0 or notrunc { return write_output(dest, data, offset, notrunc) }
  match dest.write(b"") {
    Ok(_) => {},
    Err(failure) => { gnu.error(f"error writing {gnu.quote(dest.display())}: {gnu.strerror(failure)}"); return false },
  }
  var at = 0
  while at < data.len() {
    let amount = if data.len() - at < block_size { data.len() - at } else { block_size }
    let chunk = data.slice(at, length: amount)
    if amount != block_size or ! all_zero(chunk) {
      match bytes.write_at(dest, offset + at, chunk, create: true) {
        Ok(_) => {},
        Err(failure) => { gnu.error(f"error writing {gnu.quote(dest.display())}: {gnu.strerror(failure)}"); return false },
      }
    }
    at += amount
  }
  if data.len() > 0 and data.len() % block_size == 0 and all_zero(data.slice(data.len() - block_size, length: block_size)) {
    match bytes.write_at(dest, data.len() - 1, b"\0", create: true) {
      Ok(_) => {},
      Err(failure) => { gnu.error(f"error writing {gnu.quote(dest.display())}: {gnu.strerror(failure)}"); return false },
    }
  }
  true
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  if argv.len() == 1 and argv[0] == "--help" { gnu.help(USAGE); return }
  if argv.len() == 1 and argv[0] == "--version" { gnu.write_text("dd 0.13.0\n"); return }
  let params = fields(argv)
  for arg in argv { if arg.ends_with("PB") or arg.ends_with("EB") or arg.ends_with("P") or arg.ends_with("E") { gnu.error("memory exhausted"); exit 1 } }
  for arg in argv {
    let equal = arg.find("=")
    if equal != null {
      let key = arg.byte_slice(0, equal ?? 0)
      let value = arg.byte_slice((equal ?? 0) + 1)
      if key in ["bs", "ibs", "obs", "cbs", "count", "skip", "seek", "iseek", "oseek"] {
        for _ in range(zero_multiplier_count(value)) { eprint "dd: warning: '0x' is a zero multiplier; use '00x' if that is intended" }
        let parsed = parse_size(value)
        if parsed == null { invalid_number(value, size_overflows(value)) }
        if key in ["bs", "ibs", "obs", "cbs"] and (parsed ?? 0) == 0 { invalid_number(value) }
      }
    }
  }
  if params.ibs <= 0 or params.obs <= 0 or params.skip < 0 or params.seek < 0 or (params.count != null and (params.count ?? 0) < 0) {
    gnu.error("invalid number")
    exit 1
  }
  if params.count != null and ! params.count_bytes and (params.count ?? 0) > 0 and params.ibs > 0 and (params.count ?? 0) > MAX_DD_SIZE / params.ibs {
    invalid_number(last_value(argv, ["count"], "0"), true)
  }
  if ! params.skip_bytes and params.skip > 0 and params.ibs > 0 and params.skip > MAX_DD_SIZE / params.ibs {
    invalid_number(last_value(argv, ["skip", "iseek"], "0"), true)
  }
  if ! params.seek_bytes and params.seek > 0 and params.obs > 0 and params.seek > MAX_DD_SIZE / params.obs {
    invalid_number(last_value(argv, ["seek", "oseek"], "0"), true)
  }
  let skip_count = if params.skip_bytes { params.skip } else { params.skip * params.ibs }
  let copy_limit: Int? = if params.count == null { null } else if params.count_bytes { params.count ?? 0 } else { (params.count ?? 0) * params.ibs }
  if copy_limit != null and skip_count > MAX_DD_SIZE - (copy_limit ?? 0) {
    invalid_number(last_value(argv, ["skip", "iseek"], "0"), true)
  }
  let output_offset = if params.seek_bytes { params.seek } else { params.seek * params.obs }
  if params.ibs > 1048576 or params.obs > 1048576 { gnu.error("memory exhausted"); exit 1 }
  for arg in argv {
    if arg.find("=") == null { gnu.error(f"unrecognized operand {gnu.quote(arg)}"); exit 1 }
    let key = if arg.find("=") == null { "" } else { arg.byte_slice(0, arg.find("=") ?? 0) }
    if key not in ["if", "of", "bs", "ibs", "obs", "cbs", "count", "skip", "seek", "iseek", "oseek", "conv", "iflag", "oflag", "status"] and arg.find("=") != null { gnu.error(f"unrecognized operand {gnu.quote(arg)}"); exit 1 }
  }
  for conv in params.conversions {
    if conv not in ["notrunc", "sync", "swab", "ucase", "lcase", "block", "unblock", "sparse"] { gnu.error(f"invalid conversion {gnu.quote(conv)}"); exit 1 }
  }
  if "ucase" in params.conversions and "lcase" in params.conversions { gnu.error("cannot combine ucase and lcase conversions"); exit 1 }
  if ("block" in params.conversions or "unblock" in params.conversions) and params.cbs <= 0 { gnu.error("cbs must be greater than zero"); exit 1 }
  if "block" in params.conversions and "unblock" in params.conversions { gnu.error("cannot combine block and unblock conversions"); exit 1 }
  for flag in params.input_flags { if flag not in ["count_bytes", "skip_bytes", "fullblock"] { gnu.error(f"invalid input flag {gnu.quote(flag)}"); exit 1 } }
  for flag in params.output_flags { if flag not in ["count_bytes", "seek_bytes", "truncate"] { gnu.error(f"invalid output flag {gnu.quote(flag)}"); exit 1 } }
  if params.status not in ["default", "none", "noxfer"] { gnu.error(f"invalid status level {gnu.quote(params.status)}"); exit 1 }

  var source = b""
  if params.input == null or (params.input ?? "") == "-" {
    match io.stdin_bytes() { Ok(value) => source = value, Err(failure) => { gnu.error(f"error reading standard input: {gnu.strerror(failure)}"); exit 1 } }
  } else {
    let input_name = params.input ?? ""
    let input = fp"{input_name}"
    let char_device = match fs.stat(input, follow_symlinks: true) {
      Ok(meta) => meta.kind == "char",
      Err(_) => false,
    }
    if char_device {
      if params.count == null { gnu.error(f"cannot buffer unbounded input {gnu.quote(input_name)}"); exit 1 }
      let limit = copy_limit ?? 0
      let requested = skip_count + limit
      if requested > 67108864 { gnu.error("memory exhausted"); exit 1 }
      match bytes.read_at(input, 0, requested) {
        Ok(value) => source = value,
        Err(failure) => { gnu.error(f"failed to open {gnu.quote(input_name)}: {gnu.strerror(failure)}"); exit 1 },
      }
    } else {
      match input.read_bytes() {
        Ok(value) => source = value,
        Err(failure) => { gnu.error(f"failed to open {gnu.quote(input_name)}: {gnu.strerror(failure)}"); exit 1 },
      }
    }
  }

  if params.output == null and skip_count > source.len() { eprint "dd: 'standard input': cannot skip to specified offset" }
  let available = if skip_count < source.len() { source.len() - skip_count } else { 0 }
  var amount = available
  if copy_limit != null {
    let limit = copy_limit ?? 0
    amount = if limit < available { limit } else { available }
  }
  let data_start = if skip_count > source.len() { source.len() } else { skip_count }
  var data = source.slice(data_start, length: amount)
  let truncated = if "block" in params.conversions { truncated_records(data, params.cbs) } else { 0 }
  if "sync" in params.conversions and data.len() > 0 and data.len() % params.ibs != 0 {
    let pad = params.ibs - data.len() % params.ibs
    var values: List[Int] = []
    for index in range(data.len()) { values += [data.byte_at(index) ?? 0] }
    for _ in range(pad) { values += [0] }
    data = bytes.from_ints(values)?
  }
  if "swab" in params.conversions { data = swab(data) }
  if "ucase" in params.conversions { data = ascii_case(data, true) }
  if "lcase" in params.conversions { data = ascii_case(data, false) }
  if "block" in params.conversions { data = record_block(data, params.cbs) }
  if "unblock" in params.conversions { data = record_unblock(data, params.cbs) }

  var output_ok = true
  if params.output == null or (params.output ?? "") == "-" {
    if output_offset > 67108864 { gnu.error("memory exhausted"); exit 1 }
    if output_offset > 0 {
      match bytes.zero(output_offset) {
        Ok(prefix) => gnu.write_bytes(bytes.concat([prefix, data])),
        Err(_) => { gnu.error("memory exhausted"); exit 1 },
      }
    } else { gnu.write_bytes(data) }
  } else {
    let output_name = params.output ?? ""
    let output = fp"{output_name}"
    let offset = output_offset
    let output_is_fifo = match fs.stat(output, follow_symlinks: true) { Ok(meta) => meta.kind == "fifo", Err(_) => false }
    if output_is_fifo and offset > 0 and data.len() == 0 {
      match output.read_bytes() { Ok(_) => {}, Err(failure) => { gnu.error(f"failed to open {gnu.quote(output_name)}: {gnu.strerror(failure)}"); exit 1 } }
    } else {
      output_ok = if "sparse" in params.conversions { write_sparse_output(output, data, offset, params.notrunc, params.obs) } else { write_output(output, data, offset, params.notrunc) }
    }
  }

  if params.status != "none" {
    let in_full = if params.ibs == 0 { 0 } else { amount / params.ibs }
    let in_partial = if amount % params.ibs == 0 { 0 } else { 1 }
    let out_full = if params.obs == 0 { 0 } else { data.len() / params.obs }
    let out_partial = if data.len() % params.obs == 0 { 0 } else { 1 }
    eprint f"{in_full}+{in_partial} records in"
    eprint f"{out_full}+{out_partial} records out"
    if truncated == 1 { eprint "1 truncated record" } else if truncated > 1 { eprint f"{truncated} truncated records" }
    if params.status != "noxfer" { eprint f"{data.len()} bytes copied, 0.000 s, 0.0 B/s" }
  }
  if params.output == null or (params.output ?? "") == "-" {
    if let Err(failure) = io.flush_stdout() { gnu.error(gnu.strerror(failure)); exit 1 }
  }
  if ! output_ok { exit 1 }
}
