#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

pure usage(applet_name: Str, summary: Str) -> Str {
  f"usage: {applet_name} {summary}"
}

pure usage_error(applet_name: Str, summary: Str) -> Error {
  AppletError.Usage(usage(applet_name, summary))
}

type WhichOptions = {all: Bool, names: List[Str]}

proc all_matches(name: Str) [env, process, error] -> Result[List[Path]] {
  if "/" in name {
    let candidate = fp"{name}"
    if let Ok(_) = process.which(candidate.display()) {
      return [candidate]
    }
    return []
  }

  let search_path = env("PATH") ?? { |_| "" }
  var matches: List[Path] = []

  for directory in search_path.split(":") {
    let candidate = if directory == "" { fp"./{name}" } else { fp"{directory}/{name}" }
    if let Ok(_) = process.which(candidate.display()) {
      matches += [candidate]
    }
  }

  matches
}

proc main(...argv: List[Str]) [env, process, error] {
  let opts: WhichOptions = cli.applet(
    argv,
    {
      all: {
        form: "-a",
        default: false,
      },
      names: {
        form: "...NAME",
      },
    },
  )?
  let names = opts.names

  return Err(usage_error("which", "NAME...")) when names.len() == 0

  var missing = false

  for name in names {
    if opts.all {
      let found = all_matches(name)?
      if found.len() == 0 {
        missing = true
      } else {
        for match_path in found {
          print $match_path
        }
      }
    } else if let Ok(found) = process.which(name) {
      print $found
    } else {
      missing = true
    }
  }

  if missing {
    exit 1
  }
}
