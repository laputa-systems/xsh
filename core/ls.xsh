#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

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
  var show_all = false
  var list_directory_itself = false
  var long_format = false
  var recursive = false
  var indicator = ""
  var targets: List[Str] = []

  for arg in argv {
    match arg {
      "-1" => {}
      "-a" | "-A" => show_all = true
      "-d" => list_directory_itself = true
      "-l" | "-g" | "-n" | "-o" => long_format = true
      "-p" => indicator = "/"
      "-F" => indicator = "/"
      "-R" => recursive = true
      "-r" | "-s" | "-S" | "-t" | "-U" | "-h" => {}
      _ => {
        if arg.starts_with("-") {
          return Err(AppletError.Usage(f"ls: unsupported option '${arg}'"))
        }

        targets = targets.push(arg)
      }
    }
  }

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
