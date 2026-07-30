#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

type MvOptions = {
  no_target_directory: Bool,
  no_clobber: Bool,
  force: Bool,
  target: Str,
  operands: List[Str],
}

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
  let opts: MvOptions = cli.applet(
    argv,
    {
      no_target_directory: {
        form: "-T --no-target-directory",
        default: false,
      },
      no_clobber: {
        form: "-n",
        default: false,
        conflicts: "force",
      },
      force: {
        form: "-f",
        default: false,
        conflicts: "no_clobber",
      },
      ignored: {
        form: "-i",
        default: false,
      },
      target: {
        form: "-t DIR",
        default: "",
      },
      operands: {
        form: "...FILE",
      },
    },
  )?
  let no_target_directory = opts.no_target_directory
  let no_clobber = opts.no_clobber
  let target_directory = fp"${opts.target}"
  let has_target_directory = opts.target != ""
  let paths = opts.operands

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
