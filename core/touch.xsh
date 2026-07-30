#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

pure usage(applet_name: Str, summary: Str) -> Str {
  return f"usage: xsh applets/${applet_name}.xsh -- ${summary}"
}

pure usage_error(applet_name: Str, summary: Str) -> Error {
  return AppletError.Usage(usage(applet_name, summary))
}

type TouchOptions = {no_create: Bool, reference: Str, paths: List[Str]}

proc main(...argv: List[Str]) [fs, error] {
  let opts: TouchOptions = cli.applet(
    argv,
    {
      no_create: {
        form: "-c --no-create",
        default: false,
      },
      reference: {
        form: "-r FILE",
        default: "",
      },
      ignored: {
        form: "-a -m -h",
        default: false,
      },
      paths: {
        form: "...PATH",
      },
    },
  )?
  let no_create = opts.no_create
  let reference = fp"${opts.reference}"
  let has_reference = opts.reference != ""
  let paths = opts.paths

  if paths.len() == 0 {
    return Err(usage_error("touch", "[-c] [-r FILE] PATH..."))
  }

  for item in paths {
    let target = fp"${item}"
    continue when no_create and ! target.exists()?

    if has_reference {
      target.touch_from(reference)?
    } else {
      target.touch(create: ! no_create)?
    }
  }
}
