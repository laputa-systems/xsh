#!/usr/bin/env -S xsh --
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

proc main(...argv: List[Str]) [fs, error] {
  var summarize = false
  var human = false
  var all = false
  var total = false
  var apparent = false
  var megabytes = false
  var targets: List[Str] = []

  for arg in argv {
    match arg {
      "-s" | "--summarize" => summarize = true
      "-h" | "--human-readable" => human = true
      "-a" | "--all" => all = true
      "-c" | "--total" => total = true
      "-b" | "--bytes" => apparent = true
      "-k" => {}
      "-m" => megabytes = true
      "-sh" | "-hs" => {
        summarize = true
        human = true
      }
      "-ah" | "-ha" => {
        all = true
        human = true
      }
      "-ak" | "-ka" => all = true
      "-sk" | "-ks" => summarize = true
      "-sm" | "-ms" => {
        summarize = true
        megabytes = true
      }
      _ => {
        if arg.starts_with("-") {
          return Err(AppletError.Usage(f"du: unsupported option '${arg}'"))
        }

        targets = targets.push(arg)
      }
    }
  }

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
