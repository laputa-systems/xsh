pure set_option(text: Str, key: Str, value: Str) -> Str {
  var out = []
  var done = false
  for line in text.lines() {
    let words = line.replace("#", " ").fields()
    if ! done and words.len() > 0 and words[0] == key {
      out += [f"{key} {value}"]
      done = true
    } else {
      out += [line]
    }
  }

  if ! done {
    out += [f"{key} {value}"]
  }

  out.join("\n") + "\n"
}

proc edit_config(file: Path, key: Str, value: Str) {
  let before = file.read_text()?
  let after = set_option(before, key, value)
  if after == before {
    print f"{file.name()}: {key} is already {value}"
    return
  }

  let backup = fp"{file}.bak"
  fs.copy(file, backup, overwrite: true)?
  file.write_atomic(after)?
  print diff.unified(backup, file)?.text.trim()
}

let scratch = fs.tempdir()?
defer scratch.close()?
let config = fp"{scratch.host_path()?}/sshd_config"
config.write("Port 22\n#PermitRootLogin prohibit-password\nPasswordAuthentication yes\n")?

edit_config(config, "PermitRootLogin", "no")?
edit_config(config, "PermitRootLogin", "no")?
