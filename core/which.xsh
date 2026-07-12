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

proc main(...argv: List[Str]) [process, error] {
  var names: List[Str] = []

  for arg in argv {
    if arg == "-a" {} else if arg.starts_with("-") {
      return Err(reject_unsupported("which", arg))
    } else {
      names = names.push(arg)
    }
  }

  if names.len() == 0 {
    return Err(usage_error("which", "NAME..."))
  }

  var missing = false

  for name in names {
    match process.which(name) {
      Ok(found) => print $found
      Err(_) => missing = true
    }
  }

  if missing {
    abort(1)
  }
}
