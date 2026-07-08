export error AuthError = Failed(message: Str) : Usage

export type PasswdEntry = {name: Str, password: Str, uid: Int, gid: Int, gecos: Str, home: Path, shell: Str}

export type ShadowRecord = {raw: Bool, username: Str, password: Str, rest: List[Str], line: Str}

export type LookupResult = {found: Bool, user: PasswdEntry}

export type PasswordResult = {found: Bool, password: Str}

export pure dummy_user() -> PasswdEntry {
  return {
    name: "",
    password: "",
    uid: 0,
    gid: 0,
    gecos: "",
    home: p".",
    shell: "",
  }
}

export proc fail(applet_name: Str, message: Str) [io] -> Int {
  eprint f"${applet_name}: ${message}"
  return 1
}

export pure missing_option_value(applet_name: Str, flag: Str) -> Str {
  let _ = applet_name
  return f"option requires an argument -- ${flag}"
}

export pure invalid_option(flag: Str) -> Str {
  return f"invalid option -${flag}"
}

export pure unrecognized_option(flag: Str) -> Str {
  return f"unrecognized option ${flag}"
}

export pure split_fields(line: Str) -> List[Str] {
  return line.split(":")
}

export pure parse_passwd(text: Str) -> Result[List[PasswdEntry]] {
  var entries: List[PasswdEntry] = []

  for line in text.lines() {
    let fields = split_fields(line)
    continue when fields.len() < 7
    let uid = fields[2].parse_int() ?? -1
    let gid = fields[3].parse_int() ?? -1
    continue when uid < 0 or gid < 0

    entries = entries.push({
      name: fields[0],
      password: fields[1],
      uid: uid,
      gid: gid,
      gecos: fields[4],
      home: fp"${fields[5]}",
      shell: fields[6],
    })
  }

  return entries
}

export pure parse_shadow(text: Str) -> List[ShadowRecord] {
  var records: List[ShadowRecord] = []

  for line in text.lines() {
    let fields = split_fields(line)

    if fields.len() < 2 {
      records = records.push({raw: true, username: "", password: "", rest: [], line: line})
      continue
    }

    records = records.push({raw: false, username: fields[0], password: fields[1], rest: fields |> drop(2), line: ""})
  }

  return records
}

export pure render_shadow(records: List[Any]) -> Str {
  var lines: List[Str] = []

  for item in records {
    if item.raw {
      lines = lines.push(item.line)
    } else if item.rest.len() == 0 {
      lines = lines.push(f"${item.username}:${item.password}")
    } else {
      lines = lines.push(f"${item.username}:${item.password}:${item.rest.join(":")}")
    }
  }

  if lines.len() == 0 {
    return ""
  }

  return f"""${lines.join("\n")}
"""
}

export proc passwd_path() [env, error] -> Result[Path] {
  return fp"${env.get_or("XSH_PASSWD_FILE", "/etc/passwd")?}"
}

export proc shadow_path() [env, error] -> Result[Path] {
  return fp"${env.get_or("XSH_SHADOW_FILE", "/etc/shadow")?}"
}

export proc nologin_path() [env, error] -> Result[Path] {
  return fp"${env.get_or("XSH_NOLOGIN_FILE", "/etc/nologin.txt")?}"
}

export proc passwd_file_configured() [env] -> Bool {
  var found = false

  match env.get("XSH_PASSWD_FILE") {
    Ok(_) => found = true
    Err(_) => found = false
  }

  return found
}

export proc read_passwd_entries() [fs, env, error] -> Result[List[PasswdEntry]] {
  return parse_passwd(passwd_path()?.read_text()?)?
}

export proc read_shadow_records() [fs, env, error] -> Result[List[ShadowRecord]] {
  let path_value = shadow_path()?

  if ! path_value.exists()? {
    let empty: List[ShadowRecord] = []
    return empty
  }

  return parse_shadow(path_value.read_text()?)
}

export proc write_shadow_records(records: List[Any]) [fs, env, error] {
  shadow_path()?.write_atomic(render_shadow(records))?
}

export proc lookup_user(name: Str) [fs, env, error] -> Result[PasswdEntry] {
  if passwd_file_configured() {
    for entry in read_passwd_entries()? {
      if entry.name == name {
        return entry
      }
    }

    return Err(AuthError.Failed(f"unknown user ${name}"))
  }

  let account = user.lookup(name)?

  return {
    name: account.name,
    password: "x",
    uid: account.uid,
    gid: account.gid,
    gecos: "",
    home: account.home,
    shell: account.shell,
  }
}

export proc user_by_uid(uid: Int) [fs, env, error] -> Result[PasswdEntry] {
  if passwd_file_configured() {
    for entry in read_passwd_entries()? {
      if entry.uid == uid {
        return entry
      }
    }

    return Err(AuthError.Failed(f"unknown uid ${uid}"))
  }

  let account = user.by_uid(uid)?

  return {
    name: account.name,
    password: "x",
    uid: account.uid,
    gid: account.gid,
    gecos: "",
    home: account.home,
    shell: account.shell,
  }
}

export proc current_user_name() [fs, process, env, error] -> Result[Str] {
  var name = "root"

  match user_by_uid(applet.current_euid()) {
    Ok(entry) => name = entry.name
    Err(_) => name = "root"
  }

  return name
}

export pure shadow_password(records: List[Any], username: Str) -> PasswordResult {
  for item in records {
    if ! item.raw and item.username == username {
      return {found: true, password: item.password}
    }
  }

  return {found: false, password: ""}
}

export pure account_hash(user_entry: Any, records: List[Any]) -> PasswordResult {
  let shadow = shadow_password(records, user_entry.name)

  if shadow.found {
    return shadow
  }

  if user_entry.password != "" and user_entry.password != "x" {
    return {found: true, password: user_entry.password}
  }

  return {found: false, password: ""}
}

export proc authenticate(user_entry: Any) [fs, process, env, error, io] -> Result[Bool] {
  let records = read_shadow_records()?
  let credential = account_hash(user_entry, records)

  if ! credential.found {
    return Err(AuthError.Failed(f"unknown user ${user_entry.name}"))
  }

  let password = tui.read_secret("Password: ")?

  if applet.verify_password(password, credential.password) {
    return true
  }

  return Err(AuthError.Failed("incorrect password"))
}

export pure current_password(records: List[Any], passwd: List[Any], username: Str) -> PasswordResult {
  let shadow = shadow_password(records, username)

  if shadow.found {
    return shadow
  }

  for entry in passwd {
    if entry.name == username and entry.password != "" and entry.password != "x" {
      return {found: true, password: entry.password}
    }
  }

  return {found: false, password: ""}
}

export pure lock_password(password: Str) -> Str {
  if password.starts_with("!") {
    return password
  }

  return f"!${password}"
}

export pure unlock_password(password: Str) -> Str {
  if password.starts_with("!") {
    return (password.split("") |> drop(1)).join("")
  }

  return password
}

export pure shadow_rest_with_defaults(rest: List[Str], last_change: Str) -> List[Str] {
  var values = rest

  while values.len() < 7 {
    values = values.push("")
  }

  return [
    last_change,
    if values[1] == "" { "0" } else { values[1] },
    if values[2] == "" { "99999" } else { values[2] },
    if values[3] == "" { "7" } else { values[3] },
    values[4],
    values[5],
    values[6],
  ]
}

export pure upsert_shadow(records: List[Any], username: Str, password: Str, last_change: Str) -> List[Any] {
  var out: List[Any] = []
  var found = false

  for item in records {
    if ! item.raw and item.username == username {
      out = out.push({
        raw: false,
        username: username,
        password: password,
        rest: shadow_rest_with_defaults(item.rest, last_change),
        line: "",
      })

      found = true
    } else {
      out = out.push(item)
    }
  }

  if ! found {
    out = out.push(
      {raw: false, username: username, password: password, rest: [last_change, "0", "99999", "7", "", "", ""], line: ""},
    )
  }

  return out
}

export proc days_since_epoch() [time] -> Str {
  return f"${time.now() / 86400000}"
}
