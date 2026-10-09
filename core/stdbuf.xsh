#!/bin/xsh
use lib.gnu

const USAGE = """Usage: stdbuf OPTION... COMMAND
Run COMMAND, with modified buffering operations for its standard streams.

  -i, --input=MODE   adjust standard input stream buffering
  -o, --output=MODE  adjust standard output stream buffering
  -e, --error=MODE   adjust standard error stream buffering
                  MODE is 'L' for line buffered, '0' for unbuffered, or a
                  number for the buffer size.
      --help         display this help and exit
      --version      output version information and exit
"""

type StdbufOptions = {
  input: Str?,
  output: Str?,
  error: Str?,
  help: Bool,
  version: Bool,
  command: List[Str],
}

type BufferMode = {value: Str, message: Str?}

pure is_digits(text: Str) -> Bool {
  return false when text == ""

  for index in range(text.byte_len()) {
    let byte = text.byte_slice(index, length: 1)
    if "0123456789".find(byte) == null { return false }
  }

  true
}

pure parse_mode(text: Str) -> BufferMode {
  return {value: "L", message: null} when text == "L"
  return {value: "0", message: null} when text == "0"

  var digits = text
  var multiplier = 1
  var decimal = false

  if ! is_digits(text) {
    var split = 0
    while split < text.byte_len() and "0123456789".find(text.byte_slice(split, length: 1)) != null {
      split += 1
    }

    digits = text.byte_slice(0, length: split)
    let unit = text.byte_slice(split).upper()
    decimal = unit.ends_with("B") and unit != "B" and ! unit.ends_with("IB")
    let base = if unit.ends_with("IB") {
      unit.byte_slice(0, length: unit.byte_len() - 2)
    } else if unit.ends_with("B") {
      unit.byte_slice(0, length: unit.byte_len() - 1)
    } else {
      unit
    }

    if base == "" or base == "B" {
      multiplier = 1
    } else if base == "K" {
      multiplier = if decimal { 1000 } else { 1024 }
    } else if base == "M" {
      multiplier = if decimal { 1000000 } else { 1048576 }
    } else if base == "G" {
      multiplier = if decimal { 1000000000 } else { 1073741824 }
    } else if base == "T" {
      multiplier = if decimal { 1000000000000 } else { 1099511627776 }
    } else if base == "P" {
      multiplier = if decimal { 1000000000000000 } else { 1125899906842624 }
    } else if base == "E" {
      multiplier = if decimal { 1000000000000000000 } else { 1152921504606846976 }
    } else {
      let message = if base == "R" or base == "Q" or base == "Y" or base == "Z" {
        "Value too large for defined data type"
      } else {
        ""
      }
      return {value: "", message: message}
    }
  }

  return {value: "", message: "invalid number"} when ! is_digits(digits)

  let amount = digits.parse_uint() ?? -1
  return {value: "", message: "Value too large for defined data type"} when amount < 0
  return {value: "", message: "Value too large for defined data type"} when amount > 9223372036854775807 / multiplier

  {value: f"{amount * multiplier}", message: null}
}

proc stdbuf_library() [fs, env] -> Path? {
  let configured = env.get_or("LIBSTDBUF_DIR", "") ?? ""
  var choices: List[Path] = []

  if configured != "" {
    choices += [fp"{configured}/libstdbuf.so"]
  }

  choices += [
    /usr/libexec/coreutils/libstdbuf.so,
    /usr/local/libexec/coreutils/libstdbuf.so,
    /usr/lib64/coreutils/libstdbuf.so,
    /lib64/coreutils/libstdbuf.so,
    /usr/lib/coreutils/libstdbuf.so,
    /libexec/coreutils/libstdbuf.so,
  ]

  for library_path in choices {
    if library_path.exists() ?? false { return library_path }
  }

  null
}

proc command_status(command: Str) [fs, process] -> Int {
  return 126 when command == "."

  if command.find("/") != null {
    let target = fp"{command}"

    return 127 when ! (target.exists() ?? false)
    let launch = match target.metadata() {
      Ok(entry) => if entry.kind == "dir" or ! entry.executable { 126 } else { 0 },
      Err(_) => 126,
    }
    return launch
  }

  match process.which(command) {
    Ok(_) => 0
    Err(_) => 127
  }
}

proc run_command(
  command: Str,
  argv: List[Str],
  preload: Str,
  input: Str?,
  output: Str?,
  error: Str?,
) [process, error] -> Int {
  if input != null and output != null and error != null {
    let status = process.run(
      process.command_argv(
        command,
        argv,
        env: {LD_PRELOAD: preload, _STDBUF_I: input ?? "", _STDBUF_O: output ?? "", _STDBUF_E: error ?? ""},
      ),
    )?
    return status.shell_code()?
  }

  if input != null and output != null {
    let status = process.run(
      process.command_argv(command, argv, env: {LD_PRELOAD: preload, _STDBUF_I: input ?? "", _STDBUF_O: output ?? ""}),
    )?
    return status.shell_code()?
  }

  if input != null and error != null {
    let status = process.run(
      process.command_argv(command, argv, env: {LD_PRELOAD: preload, _STDBUF_I: input ?? "", _STDBUF_E: error ?? ""}),
    )?
    return status.shell_code()?
  }

  if output != null and error != null {
    let status = process.run(
      process.command_argv(command, argv, env: {LD_PRELOAD: preload, _STDBUF_O: output ?? "", _STDBUF_E: error ?? ""}),
    )?
    return status.shell_code()?
  }

  if input != null {
    let status = process.run(process.command_argv(command, argv, env: {LD_PRELOAD: preload, _STDBUF_I: input ?? ""}))?
    return status.shell_code()?
  }

  if output != null {
    let status = process.run(process.command_argv(command, argv, env: {LD_PRELOAD: preload, _STDBUF_O: output ?? ""}))?
    return status.shell_code()?
  }

  if error != null {
    let status = process.run(process.command_argv(command, argv, env: {LD_PRELOAD: preload, _STDBUF_E: error ?? ""}))?
    return status.shell_code()?
  }

  let status = process.run(process.command_argv(command, argv, env: {LD_PRELOAD: preload}))?
  status.shell_code()?
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: StdbufOptions = cli.applet(
    argv,
    {
      gnu: {status: 125, permute: false},
      input: {form: "-i --input MODE"},
      output: {form: "-o --output MODE"},
      error: {form: "-e --error MODE"},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      command: {form: "...COMMAND"},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }

  if opts.version {
    gnu.version("stdbuf")
    return
  }

  if opts.input == null and opts.output == null and opts.error == null {
    if opts.command.len() == 0 {
      gnu.missing_operand(125)
    }

    gnu.usage_error("you must specify a buffering mode option", 125)
  }

  if opts.command.len() == 0 {
    gnu.missing_operand(125)
  }

  var input: Str? = null
  var output: Str? = null
  var error: Str? = null

  if let mode = opts.input {
    let parsed = parse_mode(mode)
    if parsed.message != null {
      let message = if parsed.message == "" {
        f"invalid mode {gnu.quote_value(mode)}"
      } else {
        f"invalid mode {gnu.quote_value(mode)}: {parsed.message ?? "invalid mode"}"
      }
      gnu.error(message)
      exit 125
    }

    if parsed.value == "L" {
      gnu.usage_error("line buffering stdin is meaningless", 125)
    }

    input = parsed.value
  }

  if let mode = opts.output {
    let parsed = parse_mode(mode)
    if parsed.message != null {
      let message = if parsed.message == "" {
        f"invalid mode {gnu.quote_value(mode)}"
      } else {
        f"invalid mode {gnu.quote_value(mode)}: {parsed.message ?? "invalid mode"}"
      }
      gnu.error(message)
      exit 125
    }

    output = parsed.value
  }

  if let mode = opts.error {
    let parsed = parse_mode(mode)
    if parsed.message != null {
      let message = if parsed.message == "" {
        f"invalid mode {gnu.quote_value(mode)}"
      } else {
        f"invalid mode {gnu.quote_value(mode)}: {parsed.message ?? "invalid mode"}"
      }
      gnu.error(message)
      exit 125
    }

    error = parsed.value
  }

  let command = opts.command[0]
  let status = command_status(command)

  if status != 0 {
    let reason = if status == 127 { "No such file or directory" } else { "Permission denied" }
    gnu.error(f"failed to execute {gnu.quote(command)}: {reason}")
    exit status
  }

  let library = stdbuf_library()

  if library == null {
    gnu.error("libstdbuf.so is unavailable; buffering cannot be changed")
    exit 125
  }

  let preload = (library ?? /usr/libexec/coreutils/libstdbuf.so).display()

  if preload.find(":") != null {
    gnu.error(f"preload library path {gnu.quote(preload)} contains ':'")
    exit 125
  }

  match run_command(command, opts.command, preload, input, output, error) {
    result => exit result
  }
}
