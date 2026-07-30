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

type ReadlinkOptions = {canonicalize: Bool, paths: List[Str]}

proc main(...argv: List[Str]) [fs, error] {
  let opts: ReadlinkOptions = cli.applet(
    argv,
    {
      canonicalize: {
        form: "-f --canonicalize",
        default: false,
      },
      paths: {
        form: "...PATH",
      },
    },
  )?
  let canonicalize = opts.canonicalize
  let paths = opts.paths

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
