#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

pure usage(applet_name: Str, summary: Str) -> Str {
  return f"usage: xsh applets/${applet_name}.xsh -- ${summary}"
}

pure usage_error(applet_name: Str, summary: Str) -> Error {
  return AppletError.Usage(usage(applet_name, summary))
}

type ChownOptions = {recursive: Bool, no_dereference: Bool, dereference: Bool, operands: List[Str]}

proc main(...argv: List[Str]) [fs, error] {
  let opts: ChownOptions = cli.applet(
    argv,
    {
      recursive: {
        form: "-R --recursive",
        default: false,
      },
      no_dereference: {
        form: "-h --no-dereference",
        default: false,
        conflicts: "dereference",
      },
      dereference: {
        form: "-H -L --dereference",
        default: false,
        conflicts: "no_dereference",
      },
      ignored: {
        form: "-c -f -v --apply",
        default: false,
      },
      operands: {
        form: "...ARG",
      },
    },
  )?
  let recursive = opts.recursive
  let follow_symlinks = ! opts.no_dereference
  let operands = opts.operands

  if operands.len() < 2 {
    return Err(usage_error("chown", "[-Rh] OWNER[:GROUP] PATH..."))
  }

  let parts = operands[0].split(":")
  let owner_name = parts.get(0, "")
  let group_name = parts.get(1, "")

  let owner = if owner_name == "" {
    user.current()?
  } else {
    match owner_name.parse_int() { Ok(uid) => user.by_uid(uid)?, Err(_) => user.lookup(owner_name)? }
  }

  let group_rec = if group_name == "" {
    group.current()?
  } else {
    match group_name.parse_int() { Ok(gid) => group.by_gid(gid)?, Err(_) => group.lookup(group_name)? }
  }

  for item in operands |> drop(1) {
    let target = fp"${item}"

    if recursive and target.metadata()?.kind == "dir" {
      # chown doesn't restrict traversal and prints nothing per entry, so the
      # visit order is unobservable — let the walk stream unordered/parallel.
      let jobs = cpu.count()

      fs.walk(target)
        |> each --jobs=jobs { |entry|
          if owner_name != "" {
            fs.chown(entry.path, owner, follow_symlinks: follow_symlinks)?
          }

          if group_name != "" {
            fs.chgrp(entry.path, group_rec, follow_symlinks: follow_symlinks)?
          }
        }
    } else {
      if owner_name != "" {
        fs.chown(target, owner, follow_symlinks: follow_symlinks)?
      }

      if group_name != "" {
        fs.chgrp(target, group_rec, follow_symlinks: follow_symlinks)?
      }
    }
  }
}
