#!/bin/xsh
error AppletError = Usage : Usage

pure usage(applet_name: Str, summary: Str) -> Str {
  f"usage: xsh applets/{applet_name}.xsh -- {summary}"
}

pure usage_error(applet_name: Str, summary: Str) -> Error {
  AppletError.Usage(usage(applet_name, summary))
}

type WhichOptions = {all: Bool, names: List[Str]}

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
    if ! opts.all or "/" in name {
      if let Ok(found) = process.which(name) { print $found } else { missing = true }
      continue
    }
    var found = false
    guard let search = env.get("PATH") else {
      missing = true
      continue
    }
    for directory in search.split(":") {
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
