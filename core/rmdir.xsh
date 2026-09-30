#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

pure usage(applet_name: Str, summary: Str) -> Str {
  f"usage: xsh applets/${applet_name}.xsh -- ${summary}"
}

pure usage_error(applet_name: Str, summary: Str) -> Error {
  AppletError.Usage(usage(applet_name, summary))
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
  let {parents, targets, ..} = opts

  return Err(usage_error("rmdir", "[-p] DIR...")) when targets.len() == 0

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
