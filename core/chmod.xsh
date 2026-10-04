#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

pure usage(applet_name: Str, summary: Str) -> Str {
  f"usage: xsh applets/{applet_name}.xsh -- {summary}"
}

pure usage_error(applet_name: Str, summary: Str) -> Error {
  AppletError.Usage(usage(applet_name, summary))
}

pure digit_value(ch: Str) -> Result[Int] {
  match ch {
    "0" => 0
    "1" => 1
    "2" => 2
    "3" => 3
    "4" => 4
    "5" => 5
    "6" => 6
    "7" => 7
    _ => Err(AppletError.Usage(f"invalid mode digit '{ch}'"))
  }
}

pure octal_mode(raw: Str) -> Result[Int] {
  var mode = 0

  for ch in raw {
    let digit = digit_value(ch)?
    mode = mode * 8 + digit
  }

  mode
}

pure who_classes(who: Str) -> Str {
  if who == "" or "a" in who {
    "ugo"
  } else {
    who
  }
}

pure class_mask(who: Str) -> Int {
  var mask = 0
  let classes = who_classes(who)

  if "u" in classes {
    mask = mask + 0o4700
  }

  if "g" in classes {
    mask = mask + 0o2070
  }

  if "o" in classes {
    mask = mask + 0o1007
  }

  mask
}

pure perm_mask(perms: Str, who: Str, current: Int, is_dir: Bool) -> Int {
  var mask = 0
  let classes = who_classes(who)
  let executable = is_dir or current.bit_and(0o111) != 0

  for perm in perms {
    if "u" in classes {
      match perm {
        "r" => mask = mask + 0o400
        "w" => mask = mask + 0o200
        "x" => mask = mask + 0o100
        "X" => {
          if executable {
            mask = mask + 0o100
          }
        }
        "s" => mask = mask + 0o4000
        _ => {}
      }
    }

    if "g" in classes {
      match perm {
        "r" => mask = mask + 0o40
        "w" => mask = mask + 0o20
        "x" => mask = mask + 0o10
        "X" => {
          if executable {
            mask = mask + 0o10
          }
        }
        "s" => mask = mask + 0o2000
        _ => {}
      }
    }

    if "o" in classes {
      match perm {
        "r" => mask = mask + 0o4
        "w" => mask = mask + 0o2
        "x" => mask = mask + 0o1
        "X" => {
          if executable {
            mask = mask + 0o1
          }
        }
        "t" => mask = mask + 0o1000
        _ => {}
      }
    }
  }

  mask
}

pure add_mask(mode: Int, mask: Int) -> Int {
  mode.bit_or(mask)
}

pure remove_mask(mode: Int, mask: Int) -> Int {
  mode.clear_bits(mask)
}

pure symbolic_mode(spec: Str, current: Int, is_dir: Bool) -> Result[Int] {
  var mode = current % 4096

  for clause in spec.split(",") {
    var who = ""
    var op = ""
    var perms = ""

    for ch in clause {
      if op == "" and ch in "ugoa" {
        who = f"{who}{ch}"
      } else if op == "" and ch in "+-=" {
        op = ch
      } else {
        perms = f"{perms}{ch}"
      }
    }

    return Err(AppletError.Usage(f"unsupported mode '{spec}'")) when op == ""

    let mask = perm_mask(perms, who, current, is_dir)

    match op {
      "+" => mode = add_mask(mode, mask)
      "-" => mode = remove_mask(mode, mask)
      "=" => mode = add_mask(remove_mask(mode, class_mask(who)), mask)
      _ => return Err(AppletError.Usage(f"unsupported mode '{spec}'"))
    }
  }

  mode
}

pure mode_for(spec: Str, current: Int, is_dir: Bool) -> Result[Int] {
  if "+" in spec or "-" in spec or "=" in spec {
    return symbolic_mode(spec, current, is_dir)
  }

  octal_mode(spec)
}

type ChmodOptions = {recursive: Bool, paths: List[Str]}

proc main(...argv: List[Str]) [fs, error] {
  let opts: ChmodOptions = cli.applet(
    argv,
    {
      recursive: {
        form: "-R",
        default: false,
      },
      ignored: {
        form: "-c -f -v",
        default: false,
      },
      paths: {
        form: "...PATH",
      },
    },
  )?
  let {recursive, paths, ..} = opts

  return Err(usage_error("chmod", "[-R] MODE PATH...")) when paths.len() < 2

  let mode_spec = paths[0]

  for item in paths |> drop(1) {
    let target = fp"{item}"

    if recursive and target.metadata()?.kind == "dir" {
      # Descending path = children before parents. A non-root `chmod -R` that
      # clears a directory's execute bit would otherwise lock itself out of
      # resolving paths to that directory's children; chmod them first.
      for entry in fs.walk(target) |> sort-by(desc: true) .path {
        entry.path.chmod(mode_for(mode_spec, entry.mode, entry.kind == "dir")?)?
      }
    } else {
      let meta = target.metadata()?
      let mode = mode_for(mode_spec, meta.mode, meta.kind == "dir")?
      target.chmod(mode)?
    }
  }
}
