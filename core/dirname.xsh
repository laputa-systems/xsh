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

proc main(...argv: List[Str]) [error] {
  if argv.len() == 0 {
    return Err(usage_error("dirname", "PATH..."))
  }

  for arg in argv {
    if arg.starts_with("-") {
      return Err(reject_unsupported("dirname", arg))
    }

    print fp"${arg}".parent()
  }
}
