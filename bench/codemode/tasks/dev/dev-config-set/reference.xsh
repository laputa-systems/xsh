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

pure header(line: Str) -> Str? {
  let text = line.trim()
  if text.starts_with("[") and text.ends_with("]") and text.byte_len() >= 2 {
    return text.byte_slice(1, text.byte_len() - 2)
  }

  null
}

pure setting_key(line: Str) -> Str? {
  let text = line.trim()
  if header(line) != null or text.starts_with("#") or text.starts_with(";") {
    return null
  }

  let sides = line.split("=", 1)
  if sides.len() != 2 {
    return null
  }

  sides[0].trim()
}

pure apply(lines: List[Str], s: Setting) -> List[Str] {
  let wanted = f"{s.key} = {s.value}"
  var current: Str? = null
  var header_at: Int? = null
  var last_setting: Int? = null
  for i in range(lines.len()) {
    let name = header(lines[i])
    if name != null {
      current = name
      if name == s.section and header_at == null {
        header_at = i
      }

      continue
    }

    if current == s.section and header_at != null {
      let key = setting_key(lines[i])
      if key != null {
        if key == s.key {
          var out = lines
          out[i] = wanted
          return out
        }

        last_setting = i
      }
    }
  }

  if header_at == null {
    return lines + [f"[{s.section}]", wanted]
  }

  let at = (last_setting ?? header_at ?? 0) + 1
  lines[..at] + [wanted] + lines[at..]
}

var settings: List[Setting] = []
for argument in args {
  let setting = parse(argument)
  if setting == null {
    eprint f"usage: SECTION.KEY=VALUE ...; cannot read {argument}"
    exit 2
  }

  settings += [setting]
}

let file = p"/work/app.conf"
let before = file.read_text()?
var lines = before.lines()
for setting in settings {
  lines = apply(lines, setting)
}

let after = lines.join("\n") + "\n"
if after == before {
  print "unchanged"
} else {
  file.write_atomic(after)
  print "changed"
}
