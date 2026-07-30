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

type MkdirOptions = {parents: Bool, mode: Str, directories: List[Str]}

proc main(...argv: List[Str]) [fs, error] {
  let opts: MkdirOptions = cli.applet(
    argv,
    {
      parents: {
        form: "-p --parents",
        default: false,
      },
      mode: {
        form: "-m --mode MODE",
        default: "",
      },
      directories: {
        form: "...DIR",
      },
    },
  )?
  if opts.directories.len() == 0 {
    return Err(usage_error("mkdir", "[-p] [-m MODE] DIR..."))
  }

  let mode = if opts.mode == "" { -1 } else { common_mode(opts.mode)? }

  for item in opts.directories {
    let target = fp"${item}"
    target.mkdir(parents: opts.parents)?

    if mode >= 0 {
      target.chmod(mode)?
    }
  }
}
