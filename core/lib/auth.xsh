##! Authentication and account-file helpers for shipped core applets.
## Public authentication helper for shipped core applets.
export error AuthError = Failed(message: Str) : Usage

## Public authentication helper for shipped core applets.
export type PasswdEntry = {name: Str, password: Str, uid: Int, gid: Int, gecos: Str, home: Path, shell: Str}

## Public authentication helper for shipped core applets.
export type ShadowRecord = {raw: Bool, username: Str, password: Str, rest: List[Str], line: Str}

## Public authentication helper for shipped core applets.
export type LookupResult = {found: Bool, user: PasswdEntry}

## Public authentication helper for shipped core applets.
export type PasswordResult = {found: Bool, password: Str}

## Public authentication helper for shipped core applets.
export pure dummy_user() -> PasswdEntry {
  {
    name: "",
    password: "",
    uid: 0,
    gid: 0,
    gecos: "",
    home: p".",
    shell: "",
  }
}

## Public authentication helper for shipped core applets.
export proc fail(applet_name: Str, message: Str) [io] -> Int {
  eprint f"{applet_name}: {message}"
  1
}

## Public authentication helper for shipped core applets.
export pure missing_option_value(applet_name: Str, flag: Str) -> Str {
  let _ = applet_name
  f"option requires an argument -- {flag}"
}

## Public authentication helper for shipped core applets.
export pure invalid_option(flag: Str) -> Str {
  f"invalid option -{flag}"
}

## Public authentication helper for shipped core applets.
export pure unrecognized_option(flag: Str) -> Str {
  f"unrecognized option {flag}"
}

## Public authentication helper for shipped core applets.
export pure split_fields(line: Str) -> List[Str] {
  line.split(":")
}

## Public authentication helper for shipped core applets.
export pure parse_passwd(text: Str) -> Result[List[PasswdEntry], Error] {
  let entries: List[PasswdEntry] = collect {
    for line in text.lines() {
      let fields = split_fields(line)
      continue when fields.len() < 7
      let uid = fields[2].parse_int() ?? -1
      let gid = fields[3].parse_int() ?? -1
      continue when uid < 0 or gid < 0

      yield {
        name: fields[0],
        password: fields[1],
        uid: uid,
        gid: gid,
        gecos: fields[4],
        home: fp"{fields[5]}",
        shell: fields[6],
      }
    }
  }

  entries
}

## Public authentication helper for shipped core applets.
export pure parse_shadow(text: Str) -> List[ShadowRecord] {
  let records: List[ShadowRecord] = collect {
    for line in text.lines() {
      let fields = split_fields(line)

      if fields.len() < 2 {
        yield {raw: true, username: "", password: "", rest: [], line: line}
        continue
      }

      yield {raw: false, username: fields[0], password: fields[1], rest: fields |> drop(2), line: ""}
    }
  }

  records
}

## Public authentication helper for shipped core applets.
export pure render_shadow(records: List[ShadowRecord]) -> Str {
  let lines = collect {
    for item in records {
      if item.raw {
        yield item.line
      } else if item.rest.is_empty() {
        yield f"{item.username}:{item.password}"
      } else {
        yield f"{item.username}:{item.password}:{item.rest.join(":")}"
      }
    }
  }

  return "" when lines.is_empty()

  f"""{lines.join("\n")}
"""
}

## Public authentication helper for shipped core applets.
export proc passwd_path() [env, error] -> Result[Path, Error] {
  fp"{env.get_or("XSH_PASSWD_FILE", "/etc/passwd")?}"
}

## Public authentication helper for shipped core applets.
export proc shadow_path() [env, error] -> Result[Path, Error] {
  fp"{env.get_or("XSH_SHADOW_FILE", "/etc/shadow")?}"
}

## Public authentication helper for shipped core applets.
export proc nologin_path() [env, error] -> Result[Path, Error] {
  fp"{env.get_or("XSH_NOLOGIN_FILE", "/etc/nologin.txt")?}"
}

## Public authentication helper for shipped core applets.
export proc passwd_file_configured() [env] -> Bool {
  var found = false

  match e"XSH_PASSWD_FILE" {
    Ok(_) => found = true
    Err(_) => found = false
  }

  found
}

## Public authentication helper for shipped core applets.
export proc read_passwd_entries() [fs, env, error] -> Result[List[PasswdEntry], Error] {
  parse_passwd(passwd_path()?.read_text()?)?
}

## Public authentication helper for shipped core applets.
export proc read_shadow_records() [fs, env, error] -> Result[List[ShadowRecord], Error] {
  let path_value = shadow_path()?

  if ! path_value.exists() {
    let empty: List[ShadowRecord] = []
    return empty
  }

  parse_shadow(path_value.read_text()?)
}

## Public authentication helper for shipped core applets.
export proc write_shadow_records(records: List[ShadowRecord]) [fs, env, error] {
  shadow_path()?.write_atomic(render_shadow(records))
}

## Public authentication helper for shipped core applets.
export proc lookup_user(name: Str) [fs, env, error] -> Result[PasswdEntry, Error] {
  if passwd_file_configured() {
    for entry in read_passwd_entries()? {
      return entry when entry.name == name
    }

    return Err(AuthError.Failed(f"unknown user {name}"))
  }

  let account = user.lookup(name)?

  {
    name: account.name,
    password: "x",
    uid: account.uid,
    gid: account.gid,
    gecos: "",
    home: account.home,
    shell: account.shell,
  }
}

## Public authentication helper for shipped core applets.
export proc user_by_uid(uid: Int) [fs, env, error] -> Result[PasswdEntry, Error] {
  if passwd_file_configured() {
    for entry in read_passwd_entries()? {
      return entry when entry.uid == uid
    }

    return Err(AuthError.Failed(f"unknown uid {uid}"))
  }

  let account = user.by_uid(uid)?

  {
    name: account.name,
    password: "x",
    uid: account.uid,
    gid: account.gid,
    gecos: "",
    home: account.home,
    shell: account.shell,
  }
}

## Public authentication helper for shipped core applets.
export proc current_user_name() [fs, process, env, error] -> Result[Str, Error] {
  var name = "root"

  if let Ok(entry) = user_by_uid(applet.current_euid()) {
    name = entry.name
  } else {
    name = "root"
  }

  name
}

## Public authentication helper for shipped core applets.
export pure shadow_password(records: List[ShadowRecord], username: Str) -> PasswordResult {
  for item in records {
    if ! item.raw and item.username == username {
      return {found: true, password: item.password}
    }
  }

  {found: false, password: ""}
}

## Public authentication helper for shipped core applets.
export pure account_hash(user_entry: PasswdEntry, records: List[ShadowRecord]) -> PasswordResult {
  let shadow = shadow_password(records, user_entry.name)

  return shadow when shadow.found

  if user_entry.password != "" and user_entry.password != "x" {
    return {found: true, password: user_entry.password}
  }

  {found: false, password: ""}
}

## Public authentication helper for shipped core applets.
export proc authenticate(user_entry: PasswdEntry) [fs, process, env, error, io] -> Result[Bool, Error] {
  let records = read_shadow_records()?
  let credential = account_hash(user_entry, records)

  return Err(AuthError.Failed(f"unknown user {user_entry.name}")) unless credential.found

  let password = tui.read_secret("Password: ")?

  return true when applet.verify_password(password, credential.password)

  Err(AuthError.Failed("incorrect password"))
}

## Public authentication helper for shipped core applets.
export pure current_password(records: List[ShadowRecord], passwd: List[PasswdEntry], username: Str) -> PasswordResult {
  let shadow = shadow_password(records, username)

  return shadow when shadow.found

  for entry in passwd {
    if entry.name == username and entry.password != "" and entry.password != "x" {
      return {found: true, password: entry.password}
    }
  }

  {found: false, password: ""}
}

## Public authentication helper for shipped core applets.
export pure lock_password(password: Str) -> Str {
  return password when password.starts_with("!")

  f"!{password}"
}

## Public authentication helper for shipped core applets.
export pure unlock_password(password: Str) -> Str {
  return password.split("") |> drop(1).join("") when password.starts_with("!")

  password
}

## Public authentication helper for shipped core applets.
export pure shadow_rest_with_defaults(rest: List[Str], last_change: Str) -> List[Str] {
  var values = rest

  while values.len() < 7 {
    values += [""]
  }

  [
    last_change,
    if values[1] == "" { "0" } else { values[1] },
    if values[2] == "" { "99999" } else { values[2] },
    if values[3] == "" { "7" } else { values[3] },
    values[4],
    values[5],
    values[6],
  ]
}

## Public authentication helper for shipped core applets.
export pure upsert_shadow(
  records: List[ShadowRecord],
  username: Str,
  password: Str,
  last_change: Str,
) -> List[ShadowRecord] {
  var found = false

  let out: List[ShadowRecord] = collect {
    for item in records {
      if ! item.raw and item.username == username {
        yield {
          raw: false,
          username: username,
          password: password,
          rest: shadow_rest_with_defaults(item.rest, last_change),
          line: "",
        }

        found = true
      } else {
        yield item
      }
    }

    if ! found {
      yield {
        raw: false,
        username: username,
        password: password,
        rest: [
          last_change,
          "0",
          "99999",
          "7",
          "",
          "",
          "",
        ],
        line: "",
      }
    }
  }

  out
}

## Public authentication helper for shipped core applets.
export proc days_since_epoch() [time] -> Str {
  f"{time.now() / 86400000}"
}
