#!/usr/bin/env -S xsh --
type DfOpts = {human: Bool, portable: Bool, paths: List[Path]}

pure parse_args(argv: List[Str]) -> Result[DfOpts] {
  let options = cli.applet(
    argv,
    {
      show_human: {
        form: "-h",
        default: false,
      },
      kilobytes: {
        form: "-k",
        default: false,
      },
      portable: {
        form: "-P",
        default: false,
      },
      paths: {
        form: "...PATH",
        kind: "Path",
      },
    },
  )?
  return {human: options.show_human or ! options.kilobytes, portable: options.portable, paths: options.paths}
}

pure size_text(size_1k: Int, human: Bool) -> Str {
  if human {
    return bytes.human(size_1k * 1024)
  }

  return f"${size_1k}"
}

pure percent_text(percent: Int) -> Str {
  return f"${percent}%"
}

proc print_linux_row(mount: FsMount, human: Bool) [fs, env, error] {
  print f"${mount.filesystem} ${size_text(mount.blocks_1k, human)} ${size_text(mount.used_1k, human)} ${size_text(
    mount.available_1k,
    human,
  )} ${percent_text(mount.capacity_percent)} ${mount.mounted_on}"
}

proc print_macos_row(mount: FsMount, human: Bool) [fs, env, error] {
  print f"${mount.filesystem} ${size_text(mount.blocks_1k, human)} ${size_text(mount.used_1k, human)} ${size_text(
    mount.available_1k,
    human,
  )} ${percent_text(mount.capacity_percent)} ${mount.files_used} ${mount.files_free} ${percent_text(
    mount.files_capacity_percent,
  )} ${mount.mounted_on}"
}

proc main(...argv: List[Str]) [fs, env, error] {
  let opts = parse_args(argv)?
  let uname = system.uname()?
  let darwin = uname.sysname == "Darwin"

  let mounts = if opts.paths.len() == 0 {
    fs.mounts()? |> sort-by .mounted_on.display()
  } else {
    opts.paths
      |> map { |target|
        fs.mount_for(target.resolve()?)?
      }
  }

  if darwin and ! opts.portable {
    print "Filesystem Size Used Avail Capacity iused ifree %iused Mounted on"

    for mount in mounts {
      print_macos_row(mount, opts.human)
    }
  } else {
    let size_label = if opts.human { "Size" } else { "1024-blocks" }
    print f"Filesystem ${size_label} Used Available Capacity Mounted on"

    for mount in mounts {
      print_linux_row(mount, opts.human)
    }
  }
}
