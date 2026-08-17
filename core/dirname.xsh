#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

pure usage(applet_name: Str, summary: Str) -> Str {
  return f"usage: xsh applets/${applet_name}.xsh -- ${summary}"
}

pure usage_error(applet_name: Str, summary: Str) -> Error {
  return AppletError.Usage(usage(applet_name, summary))
}

type DirnameOptions = {paths: List[Str]}

proc main(...argv: List[Str]) [error] {
  let opts: DirnameOptions = cli.applet(argv, {paths: {form: "...PATH"}})?
  if opts.paths.len() == 0 {
    return Err(usage_error("dirname", "PATH..."))
  }

  for arg in opts.paths {
    print fp"${arg}".parent()
  }
}
