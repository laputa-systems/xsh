##! Helpers for filesystem applet modes, sizes, and lexical path handling.
error FsMiscError = Invalid : InvalidArgument

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
    else => Err(FsMiscError.Invalid(f"invalid mode digit '{ch}'"))
  }
}

pure octal_mode(raw: Str) -> Result[Int] {
  return Err(FsMiscError.Invalid(f"invalid mode '{raw}'")) when raw == ""
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
    mask = mask.bit_or(0o4700)
  }

  if "g" in classes {
    mask = mask.bit_or(0o2070)
  }

  if "o" in classes {
    mask = mask.bit_or(0o1007)
  }

  mask
}

pure perm_mask(perms: Str, who: Str, current: Int, is_dir: Bool) -> Int {
  var mask = 0
  let classes = who_classes(who)
  let executable = is_dir or current.bit_and(0o111) != 0

  for perm in perms {
    if perm in "ugo" {
      let shift = if perm == "u" { 64 } else if perm == "g" { 8 } else { 1 }
      let bits = (current / shift).bit_and(7)
      if "u" in classes { mask = mask.bit_or(bits * 64) }
      if "g" in classes { mask = mask.bit_or(bits * 8) }
      if "o" in classes { mask = mask.bit_or(bits) }
      continue
    }
    if "u" in classes {
      match perm {
        "r" => mask = mask.bit_or(0o400)
        "w" => mask = mask.bit_or(0o200)
        "x" => mask = mask.bit_or(0o100)
        "X" => {
          if executable {
            mask = mask.bit_or(0o100)
          }
        }
        "s" => mask = mask.bit_or(0o4000)
        else => {}
      }
    }

    if "g" in classes {
      match perm {
        "r" => mask = mask.bit_or(0o40)
        "w" => mask = mask.bit_or(0o20)
        "x" => mask = mask.bit_or(0o10)
        "X" => {
          if executable {
            mask = mask.bit_or(0o10)
          }
        }
        "s" => mask = mask.bit_or(0o2000)
        else => {}
      }
    }

    if "o" in classes {
      match perm {
        "r" => mask = mask.bit_or(0o4)
        "w" => mask = mask.bit_or(0o2)
        "x" => mask = mask.bit_or(0o1)
        "X" => {
          if executable {
            mask = mask.bit_or(0o1)
          }
        }
        "t" => mask = mask.bit_or(0o1000)
        else => {}
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

pure symbolic_mode(spec: Str, current: Int, is_dir: Bool, umask: Int) -> Result[Int] {
  var mode = current % 4096
  for clause in spec.split(",") {
    var who = ""
    var at = 0
    while at < clause.byte_len() and clause.byte_slice(at, length: 1) in "ugoa" {
      who += clause.byte_slice(at, length: 1)
      at += 1
    }
    return Err(FsMiscError.Invalid(f"invalid mode '{spec}'")) when at == clause.byte_len()
    while at < clause.byte_len() {
      let op = clause.byte_slice(at, length: 1)
      return Err(FsMiscError.Invalid(f"invalid mode '{spec}'")) when op not in "+-="
      at += 1
      var perms = ""
      while at < clause.byte_len() and clause.byte_slice(at, length: 1) not in "+-=" {
        perms += clause.byte_slice(at, length: 1)
        at += 1
      }
      return Err(FsMiscError.Invalid(f"invalid mode '{spec}'")) when ! rx"^[rwxXstugo]*$".matches(perms)
      return Err(FsMiscError.Invalid(f"invalid mode '{spec}'")) when perms.byte_len() > 1 and ("u" in perms or "g" in perms or "o" in perms)
      let permissions = perm_mask(perms, who, mode, is_dir)
      let mask = if who == "" { permissions.clear_bits(umask) } else { permissions }
      match op {
        "+" => mode = add_mask(mode, mask)
        "-" => mode = remove_mask(mode, mask)
        "=" => mode = add_mask(remove_mask(mode, class_mask(who)), mask)
        else => return Err(FsMiscError.Invalid(f"invalid mode '{spec}'"))
      }
    }
  }
  mode
}

## Evaluate an octal or symbolic permission mode.
export pure mode_for(spec: Str, current: Int, is_dir: Bool, umask = 0) -> Result[Int, Error] {
  if "+" in spec or "-" in spec or "=" in spec {
    return symbolic_mode(spec, current, is_dir, umask)
  }

  octal_mode(spec)
}

## Parse a byte size with binary or decimal units.
# Size arithmetic is bounded before multiplying so invalid sizes never create files.
export pure size_value(raw: Str) -> Int? {
  let text = raw.trim()
  return null when text == ""
  var at = 0
  var radix = 10
  if text.starts_with("0x") { at = 2; radix = 16 } else if text.starts_with("0b") { at = 2; radix = 2 }
  var digits = ""
  let raw_bytes = bytes.from_text(text)
  while at < text.byte_len() {
    break when (raw_bytes.byte_at(at) ?? 0) >= 128
    let ch = text.byte_slice(at, length: 1)
    let valid = if radix == 16 { rx"^[0-9A-Fa-f]$".matches(ch) } else if radix == 2 { ch in "01" } else { ch in "0123456789" }
    break when ! valid
    digits += ch
    at += 1
  }
  if radix == 10 and digits.byte_len() > 1 and digits.starts_with("0") { radix = 8 }
  let number: Result[Int, Error] = if digits == "" and radix == 10 { Ok(1) } else {
    ((if radix == 16 { "0x" } else if radix == 2 { "0b" } else if radix == 8 { "0o" } else { "" }) + digits).parse_int()
  }
  return null when number is Err(_)
  let suffix = text.byte_slice(at)
  let units = ["", "K", "M", "G", "T", "P", "E", "Z", "Y", "R", "Q"]
  var multiplier = 1
  if suffix == "b" { multiplier = 512 } else if suffix != "" {
    var valid = false
    for index in range(1, units.len()) {
      let unit = units[index]
      return null when (bytes.from_text(suffix).byte_at(0) ?? 0) >= 128
      let first = suffix.byte_slice(0, length: 1).upper()
      let rest = suffix.byte_slice(1)
      if first == unit and rest in ["", "iB", "B", "D"] {
        let base = if rest in ["B", "D"] { 1000 } else { 1024 }
        for unused in range(index) {
          if multiplier > 9223372036854775807 / base { return if (number ?? 0) == 0 { 0 } else { null } }
          multiplier *= base
        }
        valid = true
        break
      }
    }
    return null when ! valid
  }
  let value = number ?? 0
  return null when value > 9223372036854775807 / multiplier
  value * multiplier
}

## Resolve a name with normal, existing, or missing component rules.
# Components must be resolved before processing .. in physical mode; symlink
# targets are spliced into the remaining work and bounded to detect cycles.
export proc canonical(name: Str, missing: Str, logical = false, strip = false) [fs, error] -> Result[Str, Error] {
  return Err(FsMiscError.Invalid("No such file or directory")) when name == ""
  var pending = name.split("/")
  var parts: List[Str] = []
  if ! name.starts_with("/") { parts = components(fs.cwd()?.display()) }
  if logical {
    var normalized: List[Str] = []
    for part in parts.extend(pending) {
      if part == ".." { if ! normalized.is_empty() { normalized = normalized[0..normalized.len() - 1] } } else if part != "." and part != "" { normalized += [part] }
    }
    if name.ends_with("/") or name.ends_with("/.") { normalized += ["."] }
    pending = normalized
    parts = []
  }
  var links = 0
  while ! pending.is_empty() {
    let part = pending[0]
    pending = pending[1..]
    continue when part == "" or part == "."
    if part == ".." {
      if ! parts.is_empty() { parts = parts[0..parts.len() - 1] }
      continue
    }
    let current = "/" + parts.extend([part]).join("/")
    if ! strip {
      let metadata = fs.stat(fp"{current}", follow_symlinks: false)
      if let Ok(info) = metadata {
        if info.kind == "symlink" {
          links += 1
          return Err(FsMiscError.Invalid("Too many levels of symbolic links")) when links > 40
          let target = fp"{current}".readlink()?.display()
          if target.starts_with("/") { parts = [] }
          pending = target.split("/").extend(pending)
          continue
        }
        return Err(FsMiscError.Invalid("Not a directory")) when ! pending.is_empty() and info.kind != "dir" and missing != "missing"
      } else if let Err(failure) = metadata {
        return Err(failure) when missing == "existing" or (missing == "normal" and remaining_component(pending)) or (failure.errno != 2 and ! (missing == "missing" and failure.errno == 20))
      }
    } else if missing != "missing" {
      let info = fs.stat(fp"{current}", follow_symlinks: true)
      if let Ok(found) = info { return Err(FsMiscError.Invalid("Not a directory")) when ! pending.is_empty() and found.kind != "dir" }
      if let Err(failure) = info { return Err(failure) when missing == "existing" or remaining_component(pending) or failure.errno != 2 }
    }
    parts += [part]
  }
  Ok("/" + parts.join("/"))
}

## Express a canonical target relative to a canonical directory.
export pure relative(target: Str, base: Str) -> Str {
  let a = components(target)
  let b = components(base)
  var common = 0
  while common < a.len() and common < b.len() and a[common] == b[common] { common += 1 }
  var out: List[Str] = []
  for unused in range(b.len() - common) { out += [".."] }
  out = out.extend(a[common..])
  if out.is_empty() { "." } else { out.join("/") }
}

pure components(name: Str) -> List[Str] {
  var parts: List[Str] = []
  for part in name.split("/") { if part != "" { parts += [part] } }
  parts
}

## Select the final canonicalization option in command line order.
export pure canonical_mode(args: List[Str], initial: Str) -> Str {
  var mode = initial
  for arg in args {
    break when arg == "--"
    if arg == "--canonicalize" { mode = "normal" } else if arg == "--canonicalize-existing" { mode = "existing" } else if arg == "--canonicalize-missing" { mode = "missing" } else if arg.starts_with("-") and ! arg.starts_with("--") {
      for flag in arg {
        if flag == "e" { mode = "existing" } else if flag == "m" { mode = "missing" } else if flag == "f" or flag == "E" { mode = "normal" }
      }
    }
  }
  mode
}

pure remaining_component(parts: List[Str]) -> Bool {
  for part in parts { return true when part != "" }
  false
}

## Recognize range errors separately from malformed size arguments.
export pure size_overflow(raw: Str) -> Bool {
  let text = raw.trim()
  if ! rx"^(0x[0-9A-Fa-f]+|0b[01]+|[0-9]+)?([kKmMgGtTpPeEzZyYrRqQ](iB|B|D)?)?$".matches(text) or text == "" { return false }
  if rx"^0[0-9]+$".matches(text) and rx"[89]".matches(text) { return false }
  size_value(text) == null
}
