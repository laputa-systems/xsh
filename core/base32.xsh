#!/bin/xsh
use lib.gnu

const USAGE = """Usage: base32 [OPTION]... [FILE]
Base32 encode or decode FILE, or standard input, to standard output.

  -d, --decode          decode data
  -i, --ignore-garbage  when decoding, ignore non-alphabet characters
  -w, --wrap=COLS       wrap encoded lines after COLS characters (default 76)
      --help            display this help and exit
      --version         output version information and exit
"""

type Options = {decode: Bool, ignore: Bool, wrap: Str, help: Bool, version: Bool, files: List[Str]}
type Clean = {text: Str, valid: Bool}

pure raw_for(argv: List[Str], raw: List[Bytes], name: Str) -> Bytes {
  for index in range(argv.len()) {
    if argv[index] == name { return raw[index] }
  }
  bytes.from_text(name)
}

pure alphabet(byte: Int) -> Bool {
  (byte >= 65 and byte <= 90) or (byte >= 97 and byte <= 122) or (byte >= 50 and byte <= 55)
}

pure decode_text(data: Bytes, ignore: Bool) -> Clean {
  var text = ""
  var valid = true
  for index in range(data.len()) {
    let byte = data.byte_at(index) ?? 0
    let white = byte == 9 or byte == 10 or byte == 11 or byte == 12 or byte == 13 or byte == 32
    if white { continue }
    if alphabet(byte) or byte == 61 {
      text = f"{text}{data[index..index + 1].utf8() ?? ""}"
    } else if ! ignore {
      valid = false
    }
  }
  {text: text, valid: valid}
}

pure wrapped(text: Str, width: Int) -> Bytes {
  if text == "" { return b"" }
  if width == 0 { return bytes.from_text(text) }
  var lines: List[Bytes] = []
  var at = 0
  while at < text.byte_len() {
    let end = if at + width < text.byte_len() { at + width } else { text.byte_len() }
    lines += [bytes.from_text(text.byte_slice(at, end - at)), b"\n"]
    at = end
  }
  bytes.concat(lines)
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: Options = cli.applet(
    argv,
    {
      gnu: {status: 1},
      decode: {form: "-d -D --decode", default: false},
      ignore: {form: "-i --ignore-garbage", default: false},
      wrap: {form: "-w --wrap COLS", default: "76"},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      files: {form: "...FILE"},
    },
  )?

  if opts.help { gnu.help(USAGE); return }
  if opts.version { gnu.write_text("base32 (uutils coreutils) 0.13.0\n"); return }
  if opts.files.len() > 1 { gnu.extra_operand(opts.files[1]) }

  let width = match opts.wrap.parse_int() {
    Ok(value) if value >= 0 => value,
    _ => { gnu.error(f"invalid wrap size: {gnu.quote_value(opts.wrap)}"); exit 1; 0 },
  }
  let raw_args = cli.argv_bytes()
  var data = b""
  var name = "-"
  var raw_name = b"-"
  if opts.files.len() == 0 or opts.files[0] == "-" {
    match io.stdin_bytes() {
      Ok(value) => data = value,
      Err(failure) => { gnu.error(f"read error: {gnu.strerror(failure)}"); exit 1 },
    }
  } else {
    name = opts.files[0]
    raw_name = raw_for(argv, raw_args, name)
    let target = Path.parse_bytes(raw_name)?
    match target.read_bytes() {
      Ok(value) => data = value,
      Err(failure) => {
        if gnu.errno(failure) == 5 { gnu.error(f"read error: {gnu.strerror(failure)}") } else { gnu.error(f"{gnu.quote_bytes(raw_name, always: false)}: {gnu.strerror(failure)}") }
        exit 1
      },
    }
  }

  if opts.decode {
    let cleaned = decode_text(data, opts.ignore)
    if ! cleaned.valid {
      gnu.error("error: invalid input")
      exit 1
    }
    match cleaned.text.base32_decode() {
      Ok(decoded) => gnu.write_bytes(decoded),
      Err(_) => { gnu.error("error: invalid input"); exit 1 },
    }
  } else {
    gnu.write_bytes(wrapped(data.base32(), width))
  }
  if let Err(failure) = io.flush_stdout() { gnu.error(gnu.strerror(failure)); exit 1 }
}
