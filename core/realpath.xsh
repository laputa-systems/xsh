#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

pure usage(applet_name: Str, summary: Str) -> Str {
  return f"usage: xsh applets/${applet_name}.xsh -- ${summary}"
}

pure usage_error(applet_name: Str, summary: Str) -> Error {
  return AppletError.Usage(usage(applet_name, summary))
}

type RealpathOptions = {paths: List[Str]}

proc main(...argv: List[Str]) [fs, error] {
  let opts: RealpathOptions = cli.applet(argv, {paths: {form: "...PATH"}})?
  if opts.paths.len() == 0 {
    return Err(usage_error("realpath", "PATH..."))
  }

  for item in opts.paths {
    print (fp"${item}".resolve()?)
  }
}
