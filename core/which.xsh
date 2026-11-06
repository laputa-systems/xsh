#!/bin/xsh
error AppletError = Usage : Usage

pure usage(applet_name: Str, summary: Str) -> Str {
  f"usage: xsh applets/{applet_name}.xsh -- {summary}"
}

pure usage_error(applet_name: Str, summary: Str) -> Error {
  AppletError.Usage(usage(applet_name, summary))
}

type WhichOptions = {all: Bool, names: List[Str]}

const DEFAULT_PATH = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"

proc main(...argv: List[Str]) [process, fs, env, error] {
  let opts: WhichOptions = cli.applet(
    argv,
    {
      all: {
        form: "-a --all",
        default: false,
      },
      names: {
        form: "...NAME",
      },
    },
  )?
  let names = opts.names

  return Err(usage_error("which", "NAME...")) when names.is_empty()

  var missing = false

  for name in names {
    if "/" in name or (! opts.all and env.get("PATH") is Ok(_)) {
      if let Ok(found) = process.which(name) { print $found } else { missing = true }
      continue
    }
    var found = false
    let search = match env.get("PATH") {
      Ok(value) => value
      Err(_) => DEFAULT_PATH
    }
    for directory in search.split(":") {
      if found and ! opts.all { break }
      let candidate = if directory == "" { fp"./{name}" } else { fp"{directory}/{name}" }
      if let Ok(meta) = fs.stat(candidate, follow_symlinks: true) {
        if meta.kind == "file" {
          if let Ok(executable) = fs.access(candidate, execute: true) {
            if executable { print $candidate
              found = true }
          }
        }
      }
    }
    if ! found { missing = true }
  }

  if missing {
    exit 1
  }
}
