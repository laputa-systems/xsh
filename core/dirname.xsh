#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

pure usage(applet_name: Str, summary: Str) -> Str {
  f"usage: xsh applets/${applet_name}.xsh -- ${summary}"
}

pure usage_error(applet_name: Str, summary: Str) -> Error {
  AppletError.Usage(usage(applet_name, summary))
}

type DirnameOptions = {paths: List[Str]}

proc main(...argv: List[Str]) [error] {
  let opts: DirnameOptions = cli.applet(argv, {paths: {form: "...PATH"}})?
  return Err(usage_error("dirname", "PATH...")) when opts.paths.len() == 0

  for arg in opts.paths {
    print fp"${arg}".parent()
  }
}
