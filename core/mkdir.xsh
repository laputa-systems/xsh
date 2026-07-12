#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

pure usage(applet_name: Str, summary: Str) -> Str {
  return f"usage: xsh applets/${applet_name}.xsh -- ${summary}"
}

pure usage_error(applet_name: Str, summary: Str) -> Error {
  return AppletError.Usage(usage(applet_name, summary))
}

pure common_mode(raw: Str) -> Result[Int] {
  match raw {
    "0755" => 0o755
    "0644" => 0o644
    "0600" => 0o600
    "0700" => 0o700
    "755" => 0o755
    "644" => 0o644
    "600" => 0o600
    "700" => 0o700
    "493" => 493
    "420" => 420
    "384" => 384
    "448" => 448
    _ => return Err(AppletError.Usage(f"unsupported mode '${raw}'"))
  }
}

proc main(...argv: List[Str]) [fs, error] {
  if argv.len() == 0 {
    return Err(usage_error("mkdir", "[-p|--parents] DIR..."))
  }

  var start = 0
  var parents = false
  var mode = -1

  while start < argv.len() {
    let arg = argv[start]

    if arg == "-p" or arg == "--parents" {
      parents = true
      start += 1
      continue
    }

    if arg == "-m" or arg == "--mode" {
      if start + 1 >= argv.len() {
        return Err(usage_error("mkdir", "[-p] [-m MODE] DIR..."))
      }

      mode = common_mode(argv[start + 1])?
      start += 2
      continue
    }

    if arg.starts_with("--mode=") {
      mode = common_mode(arg.replace("--mode=", ""))?
      start += 1
      continue
    }

    if arg.starts_with("-m") and arg.count_chars() > 2 {
      mode = common_mode(arg.replace("-m", ""))?
      start += 1
      continue
    }

    break
  }

  if argv.len() <= start {
    return Err(usage_error("mkdir", "[-p] [-m MODE] DIR..."))
  }

  for item in argv |> drop(start) {
    let target = fp"${item}"
    target.mkdir(parents: parents)?

    if mode >= 0 {
      target.chmod(mode)?
    }
  }
}
