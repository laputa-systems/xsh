#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

pure usage(applet_name: Str, summary: Str) -> Str {
  f"usage: xsh applets/{applet_name}.xsh -- {summary}"
}

pure usage_error(applet_name: Str, summary: Str) -> Error {
  AppletError.Usage(usage(applet_name, summary))
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

  return Err(usage_error("chown", "[-Rh] OWNER[:GROUP] PATH...")) when operands.len() < 2

  let parts = operands[0].split(":")
  let owner_name = parts.get(0) ?? ""
  let group_name = parts.get(1) ?? ""

  let owner = if owner_name == "" {
    user.current()?
  } else {
    if let Ok(uid) = owner_name.parse_int() {
      user.by_uid(uid)?
    } else {
      user.lookup(owner_name)?
    }
  }

  let group_rec = if group_name == "" {
    group.current()?
  } else {
    if let Ok(gid) = group_name.parse_int() {
      group.by_gid(gid)?
    } else {
      group.lookup(group_name)?
    }
  }

  for item in operands |> drop(1) {
    let target = fp"{item}"

    if recursive and target.metadata()?.kind == "dir" {
      # The walk chooses its traversal workers; ownership changes run in the
      # order entries arrive from that walk.
      fs.walk(target)
        |> each { |entry|
          if owner_name != "" {
            fs.chown(entry.path, owner, follow_symlinks:)
          }

          if group_name != "" {
            fs.chgrp(entry.path, group_rec, follow_symlinks:)
          }
        }
    } else {
      if owner_name != "" {
        fs.chown(target, owner, follow_symlinks:)
      }

      if group_name != "" {
        fs.chgrp(target, group_rec, follow_symlinks:)
      }
    }
  }
}
