#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

pure reject_unsupported(applet_name: Str, flag: Str) -> Error {
  return AppletError.Usage(f"${applet_name}: unsupported option '${flag}'")
}

proc main(...argv: List[Str]) [process, env, error] {
  var short = false

  for arg in argv {
    match arg {
      "-s" => short = true
      "-f" => {}
      _ => return Err(reject_unsupported("hostname", arg))
    }
  }

  let name = system.hostname()?
  print ${if short { name.split(".")[0] } else { name }}
}
