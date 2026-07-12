#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

pure usage(applet_name: Str, summary: Str) -> Str {
  return f"usage: xsh applets/${applet_name}.xsh -- ${summary}"
}

pure usage_error(applet_name: Str, summary: Str) -> Error {
  return AppletError.Usage(usage(applet_name, summary))
}

proc read_text_inputs(paths: List[Str]) [fs, error, io] -> Result[Str] {
  var out = ""

  if paths.len() == 0 {
    return io.stdin_text()?
  }

  for item in paths {
    if item == "-" {
      out = f"${out}${io.stdin_text()?}"
    } else {
      out = f"${out}${fp"${item}".read_text()?}"
    }
  }

  return out
}

pure ascii_chars() -> Str {
  return """	
 !"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~
"""
}

pure expand_classes(spec: Str) -> Str {
  var out = spec
  out = out.replace("[:alnum:]", "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789")
  out = out.replace("[:alpha:]", "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz")
  out = out.replace("[:blank:]", "\t ")
  out = out.replace("[:digit:]", "0123456789")
  out = out.replace("[:lower:]", "abcdefghijklmnopqrstuvwxyz")

  out = out.replace(
    "[:space:]",
    """	
""",
  )

  out = out.replace("[:upper:]", "ABCDEFGHIJKLMNOPQRSTUVWXYZ")
  out = out.replace("[:xdigit:]", "0123456789ABCDEFabcdef")
  return out
}

pure expand_ranges(spec: Str) -> Str {
  var out = expand_classes(spec)
  out = out.replace("a-z", "abcdefghijklmnopqrstuvwxyz")
  out = out.replace("A-Z", "ABCDEFGHIJKLMNOPQRSTUVWXYZ")
  out = out.replace("0-9", "0123456789")
  return out
}

pure complement(chars: Str) -> Str {
  let expanded = expand_ranges(chars)
  var out = ""

  for ch in ascii_chars().split("") {
    if ! (ch in expanded) {
      out = f"${out}${ch}"
    }
  }

  return out
}

proc main(...argv: List[Str]) [fs, error, io] {
  var delete = false
  var squeeze = false
  var complement_set = false
  var values: List[Str] = []

  for arg in argv {
    match arg {
      "-d" => delete = true
      "-s" => squeeze = true
      "-c" | "-C" => complement_set = true
      "-ds" | "-sd" => {
        delete = true
        squeeze = true
      }
      "-cd" | "-dc" | "-Cd" | "-dC" => {
        complement_set = true
        delete = true
      }
      "-cs" | "-sc" | "-Cs" | "-sC" => {
        complement_set = true
        squeeze = true
      }
      "-cds" | "-csd" | "-dcs" | "-dsc" | "-scd" | "-sdc" => {
        complement_set = true
        delete = true
        squeeze = true
      }
      _ => values = values.push(arg)
    }
  }

  if delete {
    if values.len() < 1 or values.len() > 2 {
      return Err(usage_error("tr", "[-d|-s] SET1 [SET2] [FILE]"))
    }
  } else if values.len() < 2 or values.len() > 3 {
    return Err(usage_error("tr", "[-d|-s] SET1 SET2 [FILE]"))
  }

  let input_paths = if delete {
    if values.len() == 2 { [values[1]] } else { [] }
  } else {
    if values.len() == 3 { [values[2]] } else { [] }
  }

  let input = read_text_inputs(input_paths)?
  let set1 = if complement_set { complement(values[0]) } else { expand_ranges(values[0]) }

  if delete {
    let deleted = input.delete(set1)

    if squeeze {
      print deleted.squeeze(set1)
    } else {
      print $deleted
    }
  } else {
    let translated = input.translate(set1, expand_ranges(values[1]))

    if squeeze {
      print translated.squeeze(expand_ranges(values[1]))
    } else {
      print $translated
    }
  }
}
