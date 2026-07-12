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

proc main(...argv: List[Str]) [fs, error] {
  var no_create = false
  var reference = p""
  var has_reference = false
  var paths: List[Str] = []
  var index = 0

  while index < argv.len() {
    let arg = argv[index]

    match arg {
      "-c" | "--no-create" => no_create = true
      "-a" | "-m" | "-h" => {}
      "-r" => {
        if index + 1 >= argv.len() {
          return Err(usage_error("touch", "[-c] [-r FILE] PATH..."))
        }

        reference = fp"${argv[index + 1]}"
        has_reference = true
        index += 1
      }
      _ => {
        if arg.starts_with("-") {
          return Err(reject_unsupported("touch", arg))
        }

        paths = paths.push(arg)
      }
    }

    index += 1
  }

  if paths.len() == 0 {
    return Err(usage_error("touch", "[-c] [-r FILE] PATH..."))
  }

  for item in paths {
    let target = fp"${item}"
    continue when no_create and ! target.exists()?

    if has_reference {
      target.touch_from(reference)?
    } else {
      target.touch(create: ! no_create)?
    }
  }
}
