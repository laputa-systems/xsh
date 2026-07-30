#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

type DuOptions = {summarize: Bool, all: Bool, human: Bool, apparent: Bool, megabytes: Bool}

pure ceil_div(value: Int, unit: Int) -> Int {
  if value == 0 {
    return 0
  }

  return (value + unit - 1) / unit
}

pure entry_size(meta: FsEntry, apparent: Bool) -> Int {
  if apparent {
    if meta.kind == "dir" {
      return 0
    }

    return meta.size
  }

  return ceil_div(meta.blocks_512, 2)
}

pure size_label(size_1k: Int, opts: DuOptions) -> Str {
  if opts.apparent {
    return f"${size_1k}"
  }

  if opts.human {
    return bytes.human(size_1k * 1024)
  }

  if opts.megabytes {
    return f"${ceil_div(size_1k, 1024)}"
  }

  return f"${size_1k}"
}

proc disk_usage(target: Path, opts: DuOptions, top_level: Bool) [fs, error] -> Result[Int] {
  let meta = target.metadata()?
  var size = entry_size(meta, opts.apparent)

  if meta.kind == "dir" {
    for child in fs.children(target) |> sort-by .path {
      size += disk_usage(child.path, opts, false)?
    }
  }

  if ! opts.summarize and (opts.all or meta.kind == "dir" or top_level and meta.kind == "file") {
    print f"${size_label(size, opts)}\t${target}"
  }

  return size
}

type DuCliOptions = {
  summarize: Bool,
  human: Bool,
  all: Bool,
  total: Bool,
  apparent: Bool,
  megabytes: Bool,
  targets: List[Str],
}

proc main(...argv: List[Str]) [fs, error] {
  let cli_opts: DuCliOptions = cli.applet(
    argv,
    {
      summarize: {
        form: "-s --summarize",
        default: false,
      },
      human: {
        form: "-h --human-readable",
        default: false,
      },
      all: {
        form: "-a --all",
        default: false,
      },
      total: {
        form: "-c --total",
        default: false,
      },
      apparent: {
        form: "-b --bytes",
        default: false,
      },
      megabytes: {
        form: "-m",
        default: false,
      },
      ignored: {
        form: "-k",
        default: false,
      },
      targets: {
        form: "...PATH",
      },
    },
  )?
  let summarize = cli_opts.summarize
  let human = cli_opts.human
  let all = cli_opts.all
  let total = cli_opts.total
  let apparent = cli_opts.apparent
  let megabytes = cli_opts.megabytes
  var targets = cli_opts.targets

  if targets.len() == 0 {
    targets = ["."]
  }

  var grand_total = 0
  let opts: DuOptions = {summarize, all, human, apparent, megabytes}

  for item in targets {
    let target = fp"${item}"
    let size = disk_usage(target, opts, true)?
    grand_total += size

    if summarize {
      print f"${size_label(size, opts)}\t${target}"
    }
  }

  if total {
    print f"${size_label(grand_total, opts)}\ttotal"
  }
}
