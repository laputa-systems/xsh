#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

pure usage(applet_name: Str, summary: Str) -> Str {
  f"usage: xsh applets/{applet_name}.xsh {summary}"
}

pure usage_error(applet_name: Str, summary: Str) -> Error {
  AppletError.Usage(usage(applet_name, summary))
}

type WhichOptions = {names: List[Str]}

proc main(...argv: List[Str]) [process, error] {
  let opts: WhichOptions = cli.applet(
    argv,
    {
      ignored: {
        form: "-a",
        default: false,
      },
      names: {
        form: "...NAME",
      },
    },
  )?
  let names = opts.names

  return Err(usage_error("which", "NAME...")) when names.len() == 0

  var missing = false

  for name in names {
    if let Ok(found) = process.which(name) {
      print $found
    } else {
      missing = true
    }
  }

  if missing {
    exit 1
  }
}
