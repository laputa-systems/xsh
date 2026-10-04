#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

pure usage(applet_name: Str, summary: Str) -> Str {
  f"usage: xsh applets/{applet_name}.xsh -- {summary}"
}

pure usage_error(applet_name: Str, summary: Str) -> Error {
  AppletError.Usage(usage(applet_name, summary))
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
  let {canonicalize, paths, ..} = opts

  return Err(usage_error("readlink", "[-f] PATH...")) when paths.len() == 0

  for item in paths {
    let target = fp"{item}"

    if canonicalize {
      print (target.resolve()?)
    } else {
      print (target.readlink()?)
    }
  }
}
