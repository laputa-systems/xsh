#!/bin/xsh
pure basename_value(name: Str, suffix: Str) -> Str {
  let parts = name.split("/")
  let raw = if ! parts.is_empty() { parts[-1] } else { name }
  let base = if raw == "" and parts.len() > 1 { parts[-2] } else { raw }

  return base.replace(suffix, with: "") when suffix != "" and base.ends_with(suffix)

  base
}

type BasenameOptions = {multiple: Bool, suffix: Str, names: List[Str]}

proc main(...argv: List[Str]) [error, io] -> Result[Int] {
  let opts: BasenameOptions = cli.applet(
    argv,
    {
      multiple: {
        form: "-a",
        default: false,
      },
      suffix: {
        form: "-s SUFFIX",
        default: "",
      },
      names: {
        form: "...NAME",
      },
    },
  )?
  var multiple = opts.multiple or opts.suffix != ""
  var suffix = opts.suffix
  var names = opts.names

  return 2 when names.is_empty()

  if ! multiple {
    return 2 when names.len() > 2

    if names.len() == 2 {
      suffix = names[1]
      names = [names[0]]
    }
  }

  for name in names {
    print basename_value(name, suffix)
  }

  0
}
