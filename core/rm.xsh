#!/usr/bin/env -S xsh --
error AppletError = Usage(message: Str) : Usage

pure reject_unsupported(applet_name: Str, flag: Str) -> Error {
  return AppletError.Usage(f"${applet_name}: unsupported option '${flag}'")
}

proc remove_tree(root: Path) [fs, error] {
  for entry in fs.walk(root) |> sort-by --desc .path {
    if entry.kind == "dir" {
      entry.path.remove_dir()?
    } else {
      entry.path.remove(missing_ok: true)?
    }
  }
}

proc main(...argv: List[Str]) [fs, error] {
  var recursive = false
  var force = false
  var targets: List[Str] = []

  for arg in argv {
    match arg {
      "-f" => force = true
      "-r" | "-R" => recursive = true
      "-rf" | "-fr" | "-Rf" | "-fR" => {
        recursive = true
        force = true
      }
      _ => {
        if arg.starts_with("-") {
          return Err(reject_unsupported("rm", arg))
        }

        targets = targets.push(arg)
      }
    }
  }

  if targets.len() == 0 {
    if force {
      return
    }

    return Err(AppletError.Usage("usage: xsh applets/rm.xsh -- [-f] [-r|-R] PATH..."))
  }

  for item in targets {
    let target = fp"${item}"

    if ! target.exists()? {
      continue when force
      target.remove()?
    }

    if target.metadata()?.kind == "dir" {
      if ! recursive {
        return Err(AppletError.Usage(f"rm: '${target}' is a directory"))
      }

      remove_tree(target)?
    } else {
      target.remove(missing_ok: force)?
    }
  }
}
