#!/usr/bin/env -S xsh --
error AppletError = Usage(message: Str) : Usage

pure usage(applet_name: Str, summary: Str) -> Str {
  return f"usage: xsh applets/${applet_name}.xsh -- ${summary}"
}

pure usage_error(applet_name: Str, summary: Str) -> Error {
  return AppletError.Usage(usage(applet_name, summary))
}

proc main(...argv: List[Str]) [fs, error] {
  if argv.len() == 0 {
    return Err(usage_error("realpath", "PATH..."))
  }

  for item in argv {
    print (fp"${item}".resolve()?)
  }
}
