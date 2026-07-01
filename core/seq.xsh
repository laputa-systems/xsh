#!/usr/bin/env -S xsh --
error AppletError = Usage(message: Str) : Usage

pure usage(applet_name: Str, summary: Str) -> Str {
  return f"usage: xsh applets/${applet_name}.xsh -- ${summary}"
}

pure usage_error(applet_name: Str, summary: Str) -> Error {
  return AppletError.Usage(usage(applet_name, summary))
}

pure reject_unsupported(applet_name: Str, flag: Str) -> Error {
  return AppletError.Usage(f"${applet_name}: unsupported option '${flag}'")
}

pure require_arg(applet_name: Str, summary: Str, argv: List[Str], index: Int) -> Result[Str] {
  if argv.len() <= index {
    return Err(usage_error(applet_name, summary))
  }

  return argv[index]
}

pure common_int(raw: Str, label: Str) -> Result[Int] {
  match raw {
    "1k" | "1K" => 1024
    _ => raw.parse_int().context("usage", f"unsupported ${label} '${raw}'")?
  }
}

proc repeat_char(ch: Str, count: Int) [error, io] -> Str {
  var out = ""
  var index = 0

  while index < count {
    out = f"${out}${ch}"
    index += 1
  }

  return out
}

pure unescape(raw: Str) -> Str {
  return raw.replace("\\n", "\n").replace("\\t", "\t").replace("\\\\", "\\")
}

proc pad_equal_width(raw: Str, width: Int) [error, io] -> Str {
  let missing = width - raw.count_chars()

  if missing <= 0 {
    return raw
  }

  let padding = repeat_char("0", missing)

  if raw.starts_with("-") {
    return f"-${padding}${raw.replace("-", "")}"
  }

  return f"${padding}${raw}"
}

pure should_emit(value: Int, step: Int, last: Int) -> Bool {
  if step > 0 {
    return value <= last
  }

  return value >= last
}

proc main(...argv: List[Str]) [error, io] {
  var equal_width = false
  var separator = "\n"
  var operands: List[Str] = []
  var index = 0

  while index < argv.len() {
    let arg = argv[index]

    if arg == "-w" or arg == "--equal-width" {
      equal_width = true
    } else if arg == "-s" or arg == "--separator" {
      separator = unescape(require_arg("seq", "[-w] [-s SEP] [FIRST [STEP]] LAST", argv, index + 1)?)
      index += 1
    } else if arg.starts_with("--separator=") {
      separator = unescape(arg.replace("--separator=", ""))
    } else if arg.starts_with("-s") and arg.count_chars() > 2 {
      separator = unescape(arg.replace("-s", ""))
    } else if arg.starts_with("--") {
      return Err(reject_unsupported("seq", arg))
    } else {
      operands = operands.push(arg)
    }

    index += 1
  }

  var first = 1
  var step = 1
  var last = 0

  if operands.len() == 1 {
    last = common_int(operands[0], "last")?
  } else if operands.len() == 2 {
    first = common_int(operands[0], "first")?
    last = common_int(operands[1], "last")?
  } else if operands.len() == 3 {
    first = common_int(operands[0], "first")?
    step = common_int(operands[1], "step")?
    last = common_int(operands[2], "last")?
  } else {
    return Err(usage_error("seq", "[-w] [-s SEP] [FIRST [STEP]] LAST"))
  }

  if step == 0 {
    return Err(AppletError.Usage("seq: increment cannot be zero"))
  }

  var value = first
  var values: List[Str] = []
  var max_width = 0

  while should_emit(value, step, last) {
    let rendered = f"${value}"
    values = values.push(rendered)

    if rendered.count_chars() > max_width {
      max_width = rendered.count_chars()
    }

    value += step
  }

  var out = ""
  var pos = 0

  for rendered in values {
    let item = if equal_width { pad_equal_width(rendered, max_width) } else { rendered }

    if pos > 0 {
      out = f"${out}${separator}"
    }

    out = f"${out}${item}"
    pos += 1
  }

  if values.len() > 0 {
    io.write_stdout(f"""${out}
""")?
  }
}
