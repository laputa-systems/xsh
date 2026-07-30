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
