#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

type CpOptions = {
  recursive: Bool,
  no_clobber: Bool,
  force: Bool,
  no_target_directory: Bool,
  hardlink: Bool,
  symlink: Bool,
  target: Str,
  operands: List[Str],
}

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

proc main(...argv: List[Str]) [fs, error] {
  let opts: CpOptions = cli.applet(
    argv,
    {
      recursive: {
        form: "-R -r -a",
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
      no_target_directory: {
        form: "-T --no-target-directory",
        default: false,
      },
      hardlink: {
        form: "-l",
        default: false,
        conflicts: "symlink",
      },
      symlink: {
        form: "-s",
        default: false,
        conflicts: "hardlink",
      },
      target: {
        form: "-t DIR",
        default: "",
      },
      ignored: {
        form: "-p -d -P -H -L -i -u",
        default: false,
      },
      operands: {
        form: "...FILE",
      },
    },
  )?
  let recursive = opts.recursive
  let no_clobber = opts.no_clobber
  let no_target_directory = opts.no_target_directory
  let link_mode = if opts.symlink { "symlink" } else if opts.hardlink { "hardlink" } else { "copy" }
  let target_directory = fp"${opts.target}"
  let has_target_directory = opts.target != ""
  let paths = opts.operands

  if paths.len() < 1 or ! has_target_directory and paths.len() < 2 {
    return Err(usage_error("cp", "[-R|-r|-a|-p|-T] [-t DIR] SOURCE... DEST"))
  }

  let dest = if has_target_directory { target_directory } else { fp"${paths[paths.len() - 1]}" }
  let sources = if has_target_directory { paths } else { paths |> take(paths.len() - 1) }
  var target_is_dir = false

  if ! no_target_directory and dest.exists()? {
    target_is_dir = dest.metadata()?.kind == "dir"
  }

  if sources.len() > 1 and ! target_is_dir {
    return Err(AppletError.Usage(f"cp: target '${dest}' is not a directory"))
  }

  for source_text in sources {
    let source = fp"${source_text}"
    let source_meta = source.metadata()?
    let target = dest_for(source, dest, target_is_dir)
    continue when no_clobber and target.exists()?

    if link_mode == "symlink" {
      fs.symlink(source, target)?
    } else if link_mode == "hardlink" {
      source.hardlink(target)?
    } else if source_meta.kind == "dir" {
      if ! recursive {
        return Err(AppletError.Usage(f"cp: omitting directory '${source}'"))
      }

      fs.copy_tree(source, target, parents: true, overwrite: ! no_clobber)?
    } else {
      fs.copy(source, target, overwrite: ! no_clobber)?
    }
  }
}
