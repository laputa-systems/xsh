##! GNU-compatible diagnostics, invoked name, and byte helpers for core applets.
##!
##! Compatibility applets import this module so their messages, quoting, and
##! exit statuses match GNU coreutils exactly. Diagnostics go to stderr and
##! never decide the applet's exit status, except the usage helpers, which end
##! the script with the status the caller passes. Conventional statuses:
##! 1 for most utilities, 2 for ls, cmp, diff, and grep, and 125 for env, nice,
##! nohup, timeout, stdbuf, and chroot.
##!
##! Output helpers explicitly flush buffered writes so a closed or full
##! stdout is reported before the applet can return success.

const VERSION = "0.0.1"

## One undecodable operating-system argument retained beside its CLI token.
export type RawArgument = {marker: Str, value: Bytes}
## Text tokens for the option parser and the original bytes that need restoring.
export type PreparedArguments = {text: List[Str], raw: List[RawArgument]}

# The option parser takes text; NUL-marked placeholders preserve undecodable
# arguments because operating-system argv values cannot contain NUL.
## Prepare raw argv for the text-based option parser.
export pure prepare_arguments(argv: List[Bytes]) -> PreparedArguments {
  var text: List[Str] = []
  var raw: List[RawArgument] = []
  for index in range(argv.len()) {
    let argument = argv[index]
    match argument.utf8() {
      Ok(value) => text += [value]
      Err(_) => {
        let marker = f"\0gnu-raw-argument-{index}\0"
        text += [marker]
        raw += [{marker: marker, value: argument}]
      }
    }
  }
  {text: text, raw: raw}
}

## Restore an undecodable argument or encode an ordinary text token as bytes.
export pure argument_bytes(value: Str, raw: List[RawArgument]) -> Bytes {
  for argument in raw {
    if argument.marker == value { return argument.value }
  }
  bytes.from_text(value)
}

## The script path exactly as the kernel passed it, without resolving
## symlinks, so an alias such as `dir` or `[` sees its own name.
export proc invoked_path() [process] -> Path {
  process.script_path() ?? p"xsh"
}

## The invoked name: the final path component without a `.xsh` suffix.
export proc prog() [process] -> Str {
  let name = invoked_path().basename()
  return name.byte_slice(0, name.byte_len() - 4) when name.ends_with(".xsh") and name.byte_len() > 4

  name
}

## The command words used in `Try '... --help'` hints: `XSH_EXECUTION_PHRASE`
## when a multicall dispatcher set it, otherwise the invoked name.
export proc phrase() [process, env] -> Str {
  let named = env.get_or("XSH_EXECUTION_PHRASE", "") ?? ""

  return named when named != ""

  prog()
}

## Print `PROG: MESSAGE` to stderr.
export proc error(message: Str) [process] -> Unit {
  eprint f"{prog()}: {message}"
}

type Pieces = {texts: List[Str], kinds: List[Int]}

pure octal(byte: Int) -> Str {
  f"\\{byte / 64}{byte / 8 % 8}{byte % 8}"
}

pure control_escape(byte: Int) -> Str {
  return "\\a" when byte == 7
  return "\\b" when byte == 8
  return "\\t" when byte == 9
  return "\\n" when byte == 10
  return "\\v" when byte == 11
  return "\\f" when byte == 12
  return "\\r" when byte == 13

  octal(byte)
}

pure utf8_width(lead: Int) -> Int {
  return 2 when lead >= 194 and lead <= 223
  return 3 when lead >= 224 and lead <= 239
  return 4 when lead >= 240 and lead <= 244

  0
}

# Kinds: 0 plain, 1 forces quotes, 2 control escape, 3 apostrophe, 4 byte
# escape, 5 forces quotes and single-quote style.
pure ascii_kind(text: Str) -> Int {
  return 3 when text == "'"
  return 5 when "\"`$\\^=".find(text) != null
  return 1 when "&*()|[;<>?! ".find(text) != null

  0
}

pure decode_name(raw: Bytes, utf8: Bool) -> Pieces {
  var texts = []
  var kinds = []
  var at = 0
  let total = raw.len()

  while at < total {
    let byte = raw.byte_at(at) ?? 0
    var text = ""
    var kind = 0
    var width = 1

    if byte >= 128 {
      let need = if utf8 { utf8_width(byte) } else { 0 }
      let char = if need > 0 and at + need <= total { raw[at..at + need].utf8() ?? "" } else { "" }
      let c1_control = need == 2 and byte == 194 and (raw.byte_at(at + 1) ?? 0) >= 128 and (raw.byte_at(at + 1) ?? 0) <= 159

      if char == "" or c1_control {
        text = octal(byte)
        kind = 4
      } else {
        text = char
        width = need
      }
    } else if byte < 32 or byte == 127 {
      text = control_escape(byte)
      kind = 2
    } else {
      text = raw[at..at + 1].utf8() ?? ""
      kind = ascii_kind(text)
    }

    texts += [text]
    kinds += [kind]
    at += width
  }

  {texts: texts, kinds: kinds}
}

# GNU shell-escape quoting: single quotes, `'\''` for an embedded apostrophe,
# double quotes for a name whose only special character is an apostrophe, and
# `$'...'` for control and undecodable bytes.
pure quote_pieces(pieces: Pieces, always: Bool) -> Str {
  var single = false
  var apostrophe = false
  var escaped = false
  var must = always or pieces.texts.is_empty()

  for kind in pieces.kinds {
    if kind > 0 {
      must = true
    }

    if kind == 2 or kind == 5 {
      single = true
    }

    if kind == 3 {
      apostrophe = true
    }

    if kind == 4 {
      escaped = true
    }
  }

  if ! pieces.texts.is_empty() and (pieces.texts[0] == "~" or pieces.texts[0] == "#") {
    must = true
  }

  let double = apostrophe and ! single and ! escaped
  let mark = if double { "\"" } else { "'" }
  var out = ""
  var dollar = false

  for index in range(pieces.texts.len()) {
    let kind = pieces.kinds[index]
    let text = pieces.texts[index]

    if kind == 2 or kind == 4 {
      if ! dollar {
        out = f"{out}'$'"
        dollar = true
      }

      out = f"{out}{text}"
    } else if kind == 3 and ! double {
      out = f"{out}'\\''"
      dollar = false
    } else {
      if dollar {
        out = f"{out}''"
        dollar = false
      }

      out = f"{out}{text}"
    }
  }

  return f"{mark}{out}{mark}" when must

  out
}

# Non-ASCII bytes print as themselves only in a UTF-8 locale.
proc utf8_locale() [env] -> Bool {
  var value = ""

  for name in ["LC_ALL", "LC_CTYPE", "LANG"] {
    let found = env.get_or(name, "") ?? ""

    if found != "" {
      value = found
      break
    }
  }

  let lower = value.lower()
  lower.find("utf-8") != null or lower.find("utf8") != null
}

## Quote raw name bytes for a message the way GNU `quotearg` does. Control
## bytes and, outside a UTF-8 locale, non-ASCII bytes print as `$'\ooo'`. With
## `always: false` a name that needs no quoting is returned unchanged.
export proc quote_bytes(raw: Bytes, always = true) [env] -> Str {
  quote_pieces(decode_name(raw, utf8_locale()), always)
}

## Quote a name for a message (GNU `quoteaf`): `'name'` always.
export proc quote(name: Str) [env] -> Str {
  return f"'{name}'" when rx"^[A-Za-z0-9_.,+%@:/{}\]-][A-Za-z0-9_.,+%@:/{}\]~#-]*$".matches(name)

  quote_bytes(bytes.from_text(name))
}

## Quote a value for a message the way GNU `quote` does (locale style): `'...'`
## with C escapes (`\t`, `\n`, `\ooo`, `\\`, `\'`) and curly quotes in a UTF-8
## locale. File names use `quote`; arguments such as an invalid number or time
## interval use this.
export proc quote_value(text: Str) [env] -> Str {
  quote_value_bytes(bytes.from_text(text))
}

## `quote_value` for raw bytes; undecodable bytes print as octal.
export proc quote_value_bytes(raw: Bytes) [env] -> Str {
  let utf8 = utf8_locale()
  let pieces = decode_name(raw, utf8)
  var out = ""

  for index in range(pieces.texts.len()) {
    let text = pieces.texts[index]

    if text == "\\" {
      out = f"{out}\\\\"
    } else if text == "'" {
      out = f"{out}\\'"
    } else {
      out = f"{out}{text}"
    }
  }

  if utf8 { f"‘{out}’" } else { f"'{out}'" }
}

## Quote a name only when it needs it (GNU `quotef`).
export proc quote_maybe(name: Str) [env] -> Str {
  return name when rx"^[A-Za-z0-9_.,+%@:/{}\]-][A-Za-z0-9_.,+%@:/{}\]~#-]*$".matches(name)

  quote_bytes(bytes.from_text(name), always: false)
}

## The OS error number of a failed host operation, or 0 for other errors.
export pure errno(failure: Error) -> Int {
  let text = failure.message
  let at = text.find("(os error ") ?? -1

  return 0 when at < 0

  let rest = text.byte_slice(at + 10)
  let close = rest.find(")") ?? -1

  return 0 when close < 0

  rest.byte_slice(0, close).parse_int() ?? 0
}

## The `strerror` text of a failed host operation (`No such file or
## directory`) without the path prefix and `(os error N)` suffix. Other errors
## return their message unchanged. Normalize native libc spellings to the
## wording GNU utilities use without changing the underlying host error.
export pure strerror(failure: Error) -> Str {
  let text = failure.message
  let number = errno(failure)
  return "Invalid cross-device link" when number == 18
  return "Illegal seek" when number == 29
  return "Numerical result out of range" when number == 34
  return "Operation not supported" when number == 95
  return "Too many levels of symbolic links" when text.find("Symbolic link loop") != null

  let at = text.find(" (os error ") ?? -1

  return text when at < 0

  let parts = text.byte_slice(0, at).split(": ")
  parts[-1]
}

## Report `PROG: cannot VERB 'NAME': STRERROR`.
export proc cannot(verb: Str, name: Str, failure: Error) [process, env] -> Unit {
  error(f"cannot {verb} {quote(name)}: {strerror(failure)}")
}

## Report `PROG: cannot access 'NAME': STRERROR`.
export proc cannot_access(name: Str, failure: Error) [process, env] -> Unit {
  cannot("access", name, failure)
}

## Report `PROG: cannot open 'NAME' for MODE: STRERROR`.
export proc cannot_open(name: Str, failure: Error, mode = "reading") [process, env] -> Unit {
  error(f"cannot open {quote(name)} for {mode}: {strerror(failure)}")
}

## Report `PROG: error reading 'NAME': STRERROR`.
export proc error_reading(name: Str, failure: Error) [process, env] -> Unit {
  error(f"error reading {quote(name)}: {strerror(failure)}")
}

## Report `PROG: NAME: STRERROR` (name quoted only when needed), as `cat` and
## `cksum` do.
export proc name_error(name: Str, failure: Error) [process, env] -> Unit {
  error(f"{quote_maybe(name)}: {strerror(failure)}")
}

## Print `Try 'PHRASE --help' for more information.` to stderr.
export proc try_help() [process, env] -> Unit {
  eprint f"Try '{phrase()} --help' for more information."
}

## Report `PROG: MESSAGE` and the help hint, then end the script with `status`.
export proc usage_error(message: Str, status = 1) [process, env] -> Unit {
  error(message)
  try_help()
  exit status
}

## End the script with a `missing operand` usage error.
export proc missing_operand(status = 1) [process, env] -> Unit {
  usage_error("missing operand", status)
}

## End the script with a `missing operand after 'OPERAND'` usage error.
export proc missing_operand_after(operand: Str, status = 1) [process, env] -> Unit {
  usage_error(f"missing operand after {quote(operand)}", status)
}

## End the script with an `extra operand 'OPERAND'` usage error.
export proc extra_operand(operand: Str, status = 1) [process, env] -> Unit {
  usage_error(f"extra operand {quote(operand)}", status)
}

## The first line of `--version` output: `NAME (XSH core) VERSION`.
export pure version_text(name: Str) -> Str {
  f"{name} (XSH core) {VERSION}"
}

## A stdout write failed. A closed pipe ends the applet quietly with status
## 141, the status of a process killed by SIGPIPE, which is what GNU tools do;
## any other failure prints `PROG: write error: STRERROR` and exits 1.
export proc write_failed(failure: Error) [process, env] -> Unit {
  if errno(failure) == 32 {
    exit 141
  }

  error(f"write error: {strerror(failure)}")
  exit 1
}

## Write text to stdout, ending the applet on a write failure.
export proc write_text(text: Str) [process, env, io] -> Unit {
  if let Err(failure) = io.write_stdout(text) {
    write_failed(failure)
  }
  if let Err(failure) = io.flush_stdout() {
    write_failed(failure)
  }
}

## Write raw bytes to stdout, ending the applet on a write failure.
export proc write_bytes(data: Bytes) [process, env, io] -> Unit {
  if let Err(failure) = io.write_stdout_bytes(data) {
    write_failed(failure)
  }
  if let Err(failure) = io.flush_stdout() {
    write_failed(failure)
  }
}

## Print `--version` output: the first line from `version_text`.
export proc version(name: Str) [process, env, io] -> Unit {
  write_text(f"{version_text(name)}\n")
}

## Print `--help` text to stdout; the caller then returns for status 0.
export proc help(text: Str) [process, env, io] -> Unit {
  write_text(if text.ends_with("\n") { text } else { f"{text}\n" })
}

## Read one operand's bytes; `-` reads stdin. Callers report a failure with
## `name_error`, `cannot_open`, or `error_reading`.
export proc read_operand(name: Str) [fs, error, io] -> Result[Bytes, Error] {
  return io.stdin_bytes() when name == "-"

  fp"{name}".read_bytes()
}
