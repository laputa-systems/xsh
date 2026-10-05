#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

pure usage(applet_name: Str, summary: Str) -> Str {
  f"usage: xsh applets/{applet_name}.xsh -- {summary}"
}

pure usage_error(applet_name: Str, summary: Str) -> Error {
  AppletError.Usage(usage(applet_name, summary))
}

type RealpathOptions = {paths: List[Str]}

proc main(...argv: List[Str]) [fs, error] {
  let opts: RealpathOptions = cli.applet(argv, {paths: {form: "...PATH"}})?
  return Err(usage_error("realpath", "PATH...")) when opts.paths.is_empty()

  for item in opts.paths {
    print (fp"{item}".resolve()?)
  }
}
