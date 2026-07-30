#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

type LsOptions = {
  show_all: Bool,
  list_directory_itself: Bool,
  long_format: Bool,
  recursive: Bool,
  indicator: Bool,
  targets: List[Str],
}

proc print_entry(entry: FsEntry, long_format: Bool, indicator: Str) [fs] {
  print_entry_as(entry, entry.name, long_format, indicator)
}

proc print_entry_as(entry: FsEntry, name: Str, long_format: Bool, indicator: Str) [fs] {
  let suffix = if entry.kind == "dir" { indicator } else { "" }

  if long_format {
    print f"${entry.kind}\t${entry.size}\t${name}${suffix}"
  } else {
    print f"${name}${suffix}"
  }
}

proc list_dir(target: Path, show_all: Bool, long_format: Bool, indicator: Str) [fs, error] {
  if show_all {
    print_entry_as(target.metadata()?, ".", long_format, indicator)
    print_entry_as(target.parent().metadata()?, "..", long_format, indicator)
  }

  for entry in fs.ls(target)
    |> where show_all or ! .name.starts_with(".")
    |> sort-by .name {
    print_entry(entry, long_format, indicator)
  }
}

proc main(...argv: List[Str]) [fs, error] {
  let opts: LsOptions = cli.applet(
    argv,
    {
      show_all: {
        form: "-a -A",
        default: false,
      },
      list_directory_itself: {
        form: "-d",
        default: false,
      },
      long_format: {
        form: "-l",
        default: false,
      },
      long_aliases: {
        form: "-g -n -o",
        default: false,
      },
      indicator: {
        form: "-p -F",
        default: false,
      },
      recursive: {
        form: "-R",
        default: false,
      },
      ignored: {
        form: "-1 -r -s -S -t -U -h",
        default: false,
      },
      targets: {
        form: "...PATH",
      },
    },
  )?
  let show_all = opts.show_all
  let list_directory_itself = opts.list_directory_itself
  let long_format = opts.long_format or opts.long_aliases
  let recursive = opts.recursive
  let indicator = if opts.indicator { "/" } else { "" }
  var targets = opts.targets

  if targets.len() == 0 {
    targets = ["."]
  }

  for item in targets {
    let target = fp"${item}"
    let meta = target.metadata()?

    if list_directory_itself or meta.kind != "dir" {
      print_entry_as(meta, item, long_format, indicator)
    } else if recursive {
      for entry in fs.walk(target)
        |> where show_all or ! .name.starts_with(".")
        |> sort-by .path {
        print_entry(entry, long_format, indicator)
      }
    } else {
      list_dir(target, show_all, long_format, indicator)?
    }
  }
}
