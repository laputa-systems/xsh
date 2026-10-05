#!/bin/xsh
error AppletError = Usage : Usage

pure usage(applet_name: Str, summary: Str) -> Str {
  f"usage: xsh applets/{applet_name}.xsh -- {summary}"
}

pure usage_error(applet_name: Str, summary: Str) -> Error {
  AppletError.Usage(usage(applet_name, summary))
}

pure common_int(raw: Str, label: Str) -> Result[Int] {
  match raw {
    "1k" | "1K" => 1024
    else => raw.parse_int().context("usage", f"unsupported {label} '{raw}'")?
  }
}

proc repeat_char(ch: Str, count: Int) [error, io] -> Str {
  var out = ""
  var index = 0

  while index < count {
    out = f"{out}{ch}"
    index += 1
  }

  out
}

pure unescape(raw: Str) -> Str {
  raw.replace("\\n", with: "\n").replace("\\t", with: "\t").replace("\\\\", with: "\\")
}

proc pad_equal_width(raw: Str, width: Int) [error, io] -> Str {
  let missing = width - raw.count_chars()

  return raw when missing <= 0

  let padding = repeat_char("0", missing)

  return f"-{padding}{raw.replace("-", with: "")}" when raw.starts_with("-")

  f"{padding}{raw}"
}

pure should_emit(value: Int, step: Int, last: Int) -> Bool {
  return value <= last when step > 0

  value >= last
}

type SeqOptions = {equal_width: Bool, separator: Str, operands: List[Str]}

proc main(...argv: List[Str]) [error, io] {
  let opts: SeqOptions = cli.applet(
    argv,
    {
      equal_width: {
        form: "-w --equal-width",
        default: false,
      },
      separator: {
        form: "-s --separator SEP",
        default: "\n",
      },
      operands: {
        form: "...ARG",
      },
    },
  )?
  let equal_width = opts.equal_width
  let separator = unescape(opts.separator)
  let operands = opts.operands

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

  return Err(AppletError.Usage("seq: increment cannot be zero")) when step == 0

  var value = first
  var max_width = 0

  let values = collect {
    while should_emit(value, step, last) {
      let rendered = f"{value}"
      yield rendered

      if rendered.count_chars() > max_width {
        max_width = rendered.count_chars()
      }

      value += step
    }
  }

  var out = ""
  var pos = 0

  for rendered in values {
    let item = if equal_width { pad_equal_width(rendered, max_width) } else { rendered }

    if pos > 0 {
      out = f"{out}{separator}"
    }

    out = f"{out}{item}"
    pos += 1
  }

  if ! values.is_empty() {
    io.write_stdout(f"""{out}
""")
  }
}
