#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

proc remove_tree(root: Path) [fs, error] {
  for entry in fs.walk(root) |> sort-by --desc .path {
    if entry.kind == "dir" {
      entry.path.remove_dir()?
    } else {
      entry.path.remove(missing_ok: true)?
    }
  }
}

type RmOptions = {recursive: Bool, force: Bool, targets: List[Str]}

proc main(...argv: List[Str]) [fs, error] {
  let opts: RmOptions = cli.applet(
    argv,
    {
      recursive: {
        form: "-r -R",
        default: false,
      },
      force: {
        form: "-f",
        default: false,
      },
      targets: {
        form: "...PATH",
      },
    },
  )?
  let recursive = opts.recursive
  let force = opts.force
  let targets = opts.targets

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
