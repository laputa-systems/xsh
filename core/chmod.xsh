#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

pure usage(applet_name: Str, summary: Str) -> Str {
  return f"usage: xsh applets/${applet_name}.xsh -- ${summary}"
}

pure usage_error(applet_name: Str, summary: Str) -> Error {
  return AppletError.Usage(usage(applet_name, summary))
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
    _ => return Err(AppletError.Usage(f"invalid mode digit '${ch}'"))
  }
}

pure octal_mode(raw: Str) -> Result[Int] {
  var mode = 0

  for ch in raw.split("") {
    let digit = digit_value(ch)?
    mode = mode * 8 + digit
  }

  return mode
}

pure who_classes(who: Str) -> Str {
  return if who == "" or "a" in who { "ugo" } else { who }
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

  return mask
}

pure perm_mask(perms: Str, who: Str, current: Int, is_dir: Bool) -> Int {
  var mask = 0
  let classes = who_classes(who)
  let executable = is_dir or current % 512 % 2 == 1 or current % 64 >= 8 or current % 512 >= 64

  for perm in perms.split("") {
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

  return mask
}

pure mode_bits() -> List[Int] {
  return [
    0o4000,
    0o2000,
    0o1000,
    0o400,
    0o200,
    0o100,
    0o40,
    0o20,
    0o10,
    0o4,
    0o2,
    0o1,
  ]
}

pure has_bit(mode: Int, bit: Int) -> Bool {
  return mode / bit % 2 == 1
}

pure add_mask(mode: Int, mask: Int) -> Int {
  var out = mode

  for bit in mode_bits() {
    if has_bit(mask, bit) and ! has_bit(out, bit) {
      out += bit
    }
  }

  return out
}

pure remove_mask(mode: Int, mask: Int) -> Int {
  var out = mode

  for bit in mode_bits() {
    if has_bit(mask, bit) and has_bit(out, bit) {
      out -= bit
    }
  }

  return out
}

pure symbolic_mode(spec: Str, current: Int, is_dir: Bool) -> Result[Int] {
  var mode = current % 4096

  for clause in spec.split(",") {
    var who = ""
    var op = ""
    var perms = ""

    for ch in clause.split("") {
      if op == "" and ch in "ugoa" {
        who = f"${who}${ch}"
      } else if op == "" and ch in "+-=" {
        op = ch
      } else {
        perms = f"${perms}${ch}"
      }
    }

    if op == "" {
      return Err(AppletError.Usage(f"unsupported mode '${spec}'"))
    }

    let mask = perm_mask(perms, who, current, is_dir)

    match op {
      "+" => mode = add_mask(mode, mask)
      "-" => mode = remove_mask(mode, mask)
      "=" => mode = add_mask(remove_mask(mode, class_mask(who)), mask)
      _ => return Err(AppletError.Usage(f"unsupported mode '${spec}'"))
    }
  }

  return mode
}

pure mode_for(spec: Str, current: Int, is_dir: Bool) -> Result[Int] {
  if "+" in spec or "-" in spec or "=" in spec {
    return symbolic_mode(spec, current, is_dir)
  }

  return octal_mode(spec)
}

proc main(...argv: List[Str]) [fs, error] {
  var recursive = false
  var paths: List[Str] = []

  for arg in argv {
    match arg {
      "-R" => recursive = true
      "-c" | "-f" | "-v" => {}
      _ => paths = paths.push(arg)
    }
  }

  if paths.len() < 2 {
    return Err(usage_error("chmod", "[-R] MODE PATH..."))
  }

  let mode_spec = paths[0]

  for item in paths |> drop(1) {
    let target = fp"${item}"

    if recursive and target.metadata()?.kind == "dir" {
      # Descending path = children before parents. A non-root `chmod -R` that
      # clears a directory's execute bit would otherwise lock itself out of
      # resolving paths to that directory's children; chmod them first.
      for entry in fs.walk(target) |> sort-by --desc .path {
        entry.path.chmod(mode_for(mode_spec, entry.mode, entry.kind == "dir")?)?
      }
    } else {
      let meta = target.metadata()?
      let mode = mode_for(mode_spec, meta.mode, meta.kind == "dir")?
      target.chmod(mode)?
    }
  }
}
