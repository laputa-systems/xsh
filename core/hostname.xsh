#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

pure reject_unsupported(applet_name: Str, flag: Str) -> Error {
  return AppletError.Usage(f"${applet_name}: unsupported option '${flag}'")
}

type HostnameOptions = {short: Bool}

proc main(...argv: List[Str]) [process, env, error] {
  let opts: HostnameOptions = cli.applet(
    argv,
    {
      short: {
        form: "-s",
        default: false,
      },
      ignored: {
        form: "-f",
        default: false,
      },
    },
  )?

  let name = system.hostname()?
  print ${if opts.short { name.split(".")[0] } else { name }}
}
