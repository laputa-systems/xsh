#!/usr/bin/env -S xsh --
error AppletError = Usage(message: Str) : Usage

proc main(...argv: List[Str]) [fs, error] {
  var targets: List[Str] = []
  var parsing_flags = true

  for arg in argv {
    if parsing_flags and arg == "--" {
      parsing_flags = false
    } else if parsing_flags {
      match arg {
        "-k" | "-P" | "-kP" | "-Pk" | "--portability" => {}
        _ => {
          if arg.starts_with("-") {
            return Err(AppletError.Usage(f"df: unsupported option '${arg}'"))
          }

          targets = targets.push(arg)
        }
      }
    } else {
      targets = targets.push(arg)
    }
  }

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
