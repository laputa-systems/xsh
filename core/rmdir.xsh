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

type RmdirOptions = {parents: Bool, targets: List[Str]}

proc main(...argv: List[Str]) [fs, error] {
  let opts: RmdirOptions = cli.applet(
    argv,
    {
      parents: {
        form: "-p",
        default: false,
      },
      targets: {
        form: "...DIR",
      },
    },
  )?
  let parents = opts.parents
  let targets = opts.targets

  if targets.len() == 0 {
    return Err(usage_error("rmdir", "[-p] DIR..."))
  }

  for item in targets {
    var current = fp"${item}"
    current.remove_dir()?

    if parents {
      var parent = current.parent()

      while parent.display() != "" and parent.display() != "." and parent.display() != "/" {
        match parent.remove_dir() {
          Ok(_) => parent = parent.parent()
          Err(_) => break
        }
      }
    }
  }
}
