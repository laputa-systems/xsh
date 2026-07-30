#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

type DfOptions = {targets: List[Str]}

proc main(...argv: List[Str]) [fs, error] {
  let opts: DfOptions = cli.applet(
    argv,
    {
      ignored: {
        form: "-k -P --portability",
        default: false,
      },
      targets: {
        form: "...PATH",
      },
    },
  )?
  var targets = opts.targets

  if targets.len() == 0 {
    targets = ["."]
  }

  print "Filesystem 1024-blocks Used Available Capacity Mounted on"

  for item in targets {
    let resolved = fp"${item}".resolve()?
    let mount = fs.mount_for(resolved)?
    print f"${mount.filesystem} ${mount.blocks_1k} ${mount.used_1k} ${mount.available_1k} ${mount.capacity_percent}% ${mount.mounted_on}"
  }
}
