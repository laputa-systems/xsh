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

pure dest_for(source: Path, target: Path, target_is_dir: Bool) -> Path {
  if target_is_dir {
    return fp"${target}/${source.name()}"
  }

  return target
}

proc main(...argv: List[Str]) [fs, error] {
  var no_target_directory = false
  var no_clobber = false
  var target_directory = p""
  var has_target_directory = false
  var paths: List[Str] = []
  var index = 0

  while index < argv.len() {
    let arg = argv[index]

    match arg {
      "-T" | "--no-target-directory" => no_target_directory = true
      "-f" => no_clobber = false
      "-n" => no_clobber = true
      "-i" => {}
      "-t" => {
        if index + 1 >= argv.len() {
          return Err(usage_error("mv", "[-fT] [-t DIR] SOURCE... DEST"))
        }

        target_directory = fp"${argv[index + 1]}"
        has_target_directory = true
        index += 1
      }
      _ => {
        if arg.starts_with("-") {
          return Err(reject_unsupported("mv", arg))
        }

        paths = paths.push(arg)
      }
    }

    index += 1
  }

  if paths.len() < 1 {
    return Err(usage_error("mv", "[-fT] [-t DIR] SOURCE... DEST"))
  }

  let dest = if has_target_directory { target_directory } else { fp"${paths[paths.len() - 1]}" }
  let sources = if has_target_directory { paths } else { paths |> take(paths.len() - 1) }
  var target_is_dir = false

  if ! no_target_directory and dest.exists()? {
    target_is_dir = dest.metadata()?.kind == "dir"
  }

  if sources.len() > 1 and ! target_is_dir {
    return Err(AppletError.Usage(f"mv: target '${dest}' is not a directory"))
  }

  for source_text in sources {
    let source = fp"${source_text}"
    let target = dest_for(source, dest, target_is_dir)
    continue when no_clobber and target.exists()?
    source.rename(target, overwrite: ! no_clobber)?
  }
}
