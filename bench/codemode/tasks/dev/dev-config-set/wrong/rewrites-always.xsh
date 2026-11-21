type Setting = {section: Str, key: Str, value: Str}

pure parse(argument: Str) -> Setting? {
  let sides = argument.split("=", 1)
  if sides.len() != 2 {
    return null
  }

  let names = sides[0].split(".", 1)
  if names.len() != 2 or names[0].is_empty() or names[1].is_empty() {
    return null
  }

  Setting(section: names[0], key: names[1], value: sides[1])
}

pure apply(lines: List[Str], s: Setting) -> List[Str] {
  let wanted = f"{s.key} = {s.value}"
  var current = ""
  var out: List[Str] = []
  var done = false
  for line in lines {
    let text = line.trim()
    if text.starts_with("[") and text.ends_with("]") {
      if current == s.section and ! done {
        out += [wanted]
        done = true
      }

      current = text.byte_slice(1, text.byte_len() - 2)
      out += [line]
      continue
    }

    if ! done and current == s.section and line.split("=", 1)[0].trim() == s.key {
      out += [wanted]
      done = true
    } else {
      out += [line]
    }
  }

  if ! done {
    out += [f"[{s.section}]", wanted]
  }

  out
}

var settings: List[Setting] = []
for argument in args {
  let setting = parse(argument)
  if setting == null {
    eprint f"bad argument {argument}"
    exit 2
  }

  settings += [setting]
}

let file = p"/work/app.conf"
var lines = file.read_text()?.lines()
for setting in settings {
  lines = apply(lines, setting)
}

file.write_atomic(lines.join("\n") + "\n")
print "changed"
