#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

type DuOptions = {summarize: Bool, all: Bool, human: Bool, apparent: Bool, megabytes: Bool}

pure ceil_div(value: Int, unit: Int) -> Int {
  return 0 when value == 0

  (value + unit - 1) / unit
}

pure entry_size(meta: FsEntry, apparent: Bool) -> Int {
  if apparent {
    return 0 when meta.kind == "dir"

    return meta.size
  }

  ceil_div(meta.blocks_512, 2)
}

pure size_label(size_1k: Int, opts: DuOptions) -> Str {
  return f"${size_1k}" when opts.apparent

  return bytes.human(size_1k * 1024) when opts.human

  return f"${ceil_div(size_1k, 1024)}" when opts.megabytes

  f"${size_1k}"
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

  size
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
  let {summarize, human, all, total, apparent, megabytes, ..} = cli_opts
  var targets = cli_opts.targets

  if targets.len() == 0 {
    targets = ["."]
  }

  var grand_total = 0
  let opts: DuOptions = DuOptions(summarize:, all:, human:, apparent:, megabytes:)

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
