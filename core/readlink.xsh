#!/bin/xsh
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

proc main(...argv: List[Str]) [fs, error] {
  var canonicalize = false
  var paths: List[Str] = []

  for arg in argv {
    match arg {
      "-f" | "--canonicalize" => canonicalize = true
      _ => {
        if arg.starts_with("-") {
          return Err(reject_unsupported("readlink", arg))
        }

        paths = paths.push(arg)
      }
    }
  }

  if paths.len() == 0 {
    return Err(usage_error("readlink", "[-f] PATH..."))
  }

  for item in paths {
    let target = fp"${item}"

    if canonicalize {
      print (target.resolve()?)
    } else {
      print (target.readlink()?)
    }
  }
}
