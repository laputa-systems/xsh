#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

pure usage(applet_name: Str, summary: Str) -> Str {
  f"usage: xsh applets/{applet_name}.xsh -- {summary}"
}

pure usage_error(applet_name: Str, summary: Str) -> Error {
  AppletError.Usage(usage(applet_name, summary))
}

type ChgrpOptions = {recursive: Bool, no_dereference: Bool, dereference: Bool, operands: List[Str]}

proc main(...argv: List[Str]) [fs, error] {
  let opts: ChgrpOptions = cli.applet(
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

  return Err(usage_error("chgrp", "[-Rh] GROUP PATH...")) when operands.len() < 2

  let group_rec = if let Ok(gid) = operands[0].parse_int() {
    group.by_gid(gid)?
  } else {
    group.lookup(operands[0])?
  }

  for item in operands |> drop(1) {
    let target = fp"{item}"

    if recursive and target.metadata()?.kind == "dir" {
      # The walk chooses its traversal workers; group changes run in the
      # order entries arrive from that walk.
      fs.walk(target)
        |> each { |entry|
          fs.chgrp(entry.path, group_rec, follow_symlinks:)
        }
    } else {
      fs.chgrp(target, group_rec, follow_symlinks:)
    }
  }
}
