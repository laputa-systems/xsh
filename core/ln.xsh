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
  var symbolic = false
  var force = false
  var no_target_directory = false
  var paths: List[Str] = []

  for arg in argv {
    match arg {
      "-s" => symbolic = true
      "-f" => force = true
      "-n" => {}
      "-T" | "--no-target-directory" => no_target_directory = true
      "-sf" | "-fs" | "-sfn" | "-snf" | "-fns" | "-fsn" | "-nsf" | "-nfs" => {
        symbolic = true
        force = true
      }
      _ => {
        if arg.starts_with("-") {
          return Err(reject_unsupported("ln", arg))
        }

        paths = paths.push(arg)
      }
    }
  }

  if paths.len() < 2 {
    return Err(usage_error("ln", "[-sfnT] SOURCE... DEST"))
  }

  let dest = fp"${paths[paths.len() - 1]}"
  let sources = paths |> take(paths.len() - 1)
  var target_is_dir = false

  if ! no_target_directory and dest.exists()? {
    target_is_dir = dest.metadata()?.kind == "dir"
  }

  if sources.len() > 1 and ! target_is_dir {
    return Err(AppletError.Usage(f"ln: target '${dest}' is not a directory"))
  }

  for source_text in sources {
    let source = fp"${source_text}"
    let target = dest_for(source, dest, target_is_dir)

    if force {
      target.remove(missing_ok: true)?
    }

    if symbolic {
      fs.symlink(source, target)?
    } else {
      source.hardlink(target)?
    }
  }
}
