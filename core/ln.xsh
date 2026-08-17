#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

pure usage(applet_name: Str, summary: Str) -> Str {
  return f"usage: xsh applets/${applet_name}.xsh -- ${summary}"
}

pure usage_error(applet_name: Str, summary: Str) -> Error {
  return AppletError.Usage(usage(applet_name, summary))
}

pure dest_for(source: Path, target: Path, target_is_dir: Bool) -> Path {
  if target_is_dir {
    return fp"${target}/${source.name()}"
  }

  return target
}

type LnOptions = {symbolic: Bool, force: Bool, no_target_directory: Bool, paths: List[Str]}

proc main(...argv: List[Str]) [fs, error] {
  let opts: LnOptions = cli.applet(
    argv,
    {
      symbolic: {
        form: "-s",
        default: false,
      },
      force: {
        form: "-f",
        default: false,
      },
      no_target_directory: {
        form: "-T --no-target-directory",
        default: false,
      },
      ignored: {
        form: "-n",
        default: false,
      },
      paths: {
        form: "...PATH",
      },
    },
  )?
  let symbolic = opts.symbolic
  let force = opts.force
  let no_target_directory = opts.no_target_directory
  let paths = opts.paths

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
