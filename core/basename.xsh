#!/usr/bin/env -S xsh --
pure basename_value(name: Str, suffix: Str) -> Str {
  let parts = name.split("/")
  let raw = if parts.len() > 0 { parts[parts.len() - 1] } else { name }
  let base = if raw == "" and parts.len() > 1 { parts[parts.len() - 2] } else { raw }

  if suffix != "" and base.ends_with(suffix) {
    return base.replace(suffix, "")
  }

  return base
}

proc main(...argv: List[Str]) [io] -> Int {
  var multiple = false
  var suffix = ""
  var names: List[Str] = []
  var index = 0

  while index < argv.len() {
    let arg = argv[index]

    match arg {
      "-a" => multiple = true
      "-s" => {
        if index + 1 >= argv.len() {
          return 2
        }

        suffix = argv[index + 1]
        multiple = true
        index += 1
      }
      _ => {
        if arg.starts_with("-s") and arg.count_chars() > 2 {
          suffix = arg.replace("-s", "")
          multiple = true
        } else {
          if arg.starts_with("-") {
            return 2
          }

          names = names.push(arg)
        }
      }
    }

    index += 1
  }

  if names.len() == 0 {
    return 2
  }

  if ! multiple {
    if names.len() > 2 {
      return 2
    }

    if names.len() == 2 {
      suffix = names[1]
      names = [names[0]]
    }
  }

  for name in names {
    print basename_value(name, suffix)
  }

  return 0
}
