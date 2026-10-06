##! Account database parsing and login policy shared by account applets.
use auth

## A database row's searchable keys and conventional output representation.
export type LookupRow = {keys: List[Str], text: Str}

## One local group with its password marker and supplemental memberships.
export type GroupEntry = {name: Str, password: Str, gid: Int, members: List[Str]}

## Account policy rejects unsupported databases and unsafe fixture identities.
export error AccountError = Database(message: Str) | Policy(message: Str)

pure pad_right(text: Str, width: Int) -> Str {
  var result = text
  while result.byte_len() < width { result += " " }
  result
}

pure pad_left(text: Str, width: Int) -> Str {
  var result = text
  while result.byte_len() < width { result = " " + result }
  result
}

pure id_valid(value: Int) -> Bool { value >= 0 and value < 4294967295 }

## Parse valid group rows, preserving empty membership lists.
export pure parse_groups(text: Str) -> List[GroupEntry] {
  var entries: List[GroupEntry] = []
  for line in text.lines() {
    if line.starts_with("#") { continue }
    let fields = line.split(":")
    if fields.len() != 4 { continue }
    let gid = fields[2].parse_int() ?? -1
    if fields[0] == "" or !id_valid(gid) { continue }
    entries += [{name: fields[0], password: fields[1], gid: gid, members: if fields[3] == "" { [] } else { fields[3].split(",") }}]
  }
  entries
}

## Parse the supported local databases into rows for enumeration or keyed lookup.
export pure lookup_rows(database: Str, text: Str) -> Result[List[LookupRow], Error] {
  var rows: List[LookupRow] = []
  if database == "passwd" {
    for entry in auth.parse_passwd(text)? {
      if !id_valid(entry.uid) or !id_valid(entry.gid) or entry.name == "" { continue }
      rows += [{keys: [entry.name, f"{entry.uid}"], text: f"{entry.name}:{entry.password}:{entry.uid}:{entry.gid}:{entry.gecos}:{entry.home}:{entry.shell}"}]
    }
    return Ok(rows)
  }
  if database == "group" {
    for entry in parse_groups(text) { rows += [{keys: [entry.name, f"{entry.gid}"], text: f"{entry.name}:{entry.password}:{entry.gid}:{entry.members.join(",")}"}] }
    return Ok(rows)
  }
  if database != "hosts" and database != "services" and database != "protocols" {
    return Err(AccountError.Database(message: f"unknown database {database}"))
  }
  for line in text.lines() {
    let body = line.split("#", maxsplit: 1)[0].trim()
    let fields = body.words().collect()
    if fields.len() < 2 { continue }
    if database == "hosts" {
      let aliases = fields |> drop(1)
      rows += [{keys: [fields[0]].extend(aliases |> map .lower()), text: pad_right(fields[0], 15) + " " + aliases.join(" ")}]
    } else if database == "services" {
      let endpoint = fields[1].split("/")
      if endpoint.len() != 2 { continue }
      let port = endpoint[0].parse_int() ?? -1
      if port < 0 or port > 65535 { continue }
      let aliases = [fields[0]].extend(fields |> drop(2))
      var keys = [f"{port}", f"{port}/{endpoint[1]}"]
      for name in aliases { keys += [name, f"{name}/{endpoint[1]}"] }
      rows += [{keys: keys, text: pad_right(fields[0], 21) + " " + pad_left(f"{port}", 5) + f"/{endpoint[1]}" + (if fields.len() > 2 { " " + (fields |> drop(2)).join(" ") } else { "" })}]
    } else {
      let number = fields[1].parse_int() ?? -1
      if number < 0 { continue }
      rows += [{keys: [fields[0], f"{number}"].extend(fields |> drop(2)), text: pad_right(fields[0], 21) + " " + pad_left(f"{number}", 3) + (if fields.len() > 2 { " " + (fields |> drop(2)).join(" ") } else { "" })}]
    }
  }
  Ok(rows)
}

## Choose public local database files, retaining the existing passwd/group
## fixture names. These overrides select data for read-only lookups.
export proc database_file(database: Str) [env] -> Result[Path, Error] {
  match database {
    "passwd" => Ok(fp"{env.get_or("XSH_PASSWD_FILE", "/etc/passwd") ?? "/etc/passwd"}")
    "group" => Ok(fp"{env.get_or("XSH_GROUP_FILE", "/etc/group") ?? "/etc/group"}")
    "hosts" => Ok(fp"{env.get_or("XSH_HOSTS_FILE", "/etc/hosts") ?? "/etc/hosts"}")
    "services" => Ok(fp"{env.get_or("XSH_SERVICES_FILE", "/etc/services") ?? "/etc/services"}")
    "protocols" => Ok(fp"{env.get_or("XSH_PROTOCOLS_FILE", "/etc/protocols") ?? "/etc/protocols"}")
    _ => Err(AccountError.Database(message: f"unknown database {database}"))
  }
}

pure numeric_address(value: Str) -> Bool {
  if value.find(":") != null { return true }
  let fields = value.split(".")
  if fields.len() != 4 { return false }
  for field in fields {
    if !rx"^[0-9]+$".matches(field) { return false }
    let part = field.parse_int() ?? -1
    if part < 0 or part > 255 { return false }
  }
  true
}

## Resolve keyed hosts through typed forward or reverse host lookup.
export proc resolve_hosts(key: Str) [net, error] -> Result[List[Str], Error] {
  var lines: List[Str] = []
  if numeric_address(key) {
    for name in dns.reverse(key)? { lines += [pad_right(key, 15) + " " + name] }
  } else {
    for item in dns.resolve_host(key)? { lines += [pad_right(item.addr, 15) + " " + item.name] }
  }
  Ok(lines)
}

## Resolve login accounts. Privileged callers read canonical files, while
## unprivileged fixture accounts must retain the caller's actual identity.
export proc login_user(name: Str, identity: UnixId) [fs, env, process, error] -> Result[auth.PasswdEntry, Error] {
  var entry: auth.PasswdEntry = auth.dummy_user()
  if applet.current_euid() == 0 {
    env XSH_PASSWD_FILE=/etc/passwd { entry = auth.lookup_user(name)? }
  } else { entry = auth.lookup_user(name)? }
  if applet.current_euid() != 0 and (entry.uid != identity.uid or entry.uid != applet.current_euid() or entry.gid != identity.gid) {
    return Err(AccountError.Policy(message: "fixture identity must match the caller"))
  }
  if !id_valid(entry.uid) or !id_valid(entry.gid) { return Err(AccountError.Policy(message: "invalid account identity")) }
  if entry.shell == "" or !entry.shell.starts_with("/") { return Err(AccountError.Policy(message: "account requires an absolute login shell")) }
  if !entry.home.display().starts_with("/") { return Err(AccountError.Policy(message: "account requires an absolute home directory")) }
  Ok(entry)
}

## Authenticate with the shared password helper; privileged callers cannot
## replace shadow data through the inherited fixture environment.
export proc authenticate(entry: auth.PasswdEntry) [fs, env, process, error, io] -> Result[Bool, Error] {
  var verified = false
  if applet.current_euid() == 0 {
    env XSH_SHADOW_FILE=/etc/shadow { verified = auth.authenticate(entry)? }
  } else { verified = auth.authenticate(entry)? }
  Ok(verified)
}

pure age_field(text: Str) -> Result[Int] {
  if text == "" or text == "-1" { return Ok(-1) }
  let value = text.parse_int()?
  if value < 0 { return Err(AccountError.Policy(message: "invalid shadow age field")) }
  Ok(value)
}

## Check account expiry without granting authentication from file contents.
export proc check_expiration(entry: auth.PasswdEntry) [fs, env, process, time, error] -> Result[Unit, Error] {
  var records: List[auth.ShadowRecord] = []
  if applet.current_euid() == 0 {
    env XSH_SHADOW_FILE=/etc/shadow { records = auth.read_shadow_records()? }
  } else { records = auth.read_shadow_records()? }
  let today = time.now() / 86400000
  for item in records {
    if item.raw or item.username != entry.name { continue }
    let expiration = age_field(item.rest.get(5) ?? "")?
    if expiration >= 0 and today >= expiration { return Err(AccountError.Policy(message: "account has expired")) }
    let changed = age_field(item.rest.get(0) ?? "")?
    let maximum = age_field(item.rest.get(2) ?? "")?
    if changed == 0 or (changed > 0 and maximum >= 0 and today - changed > maximum) {
      return Err(AccountError.Policy(message: "password has expired; password changes during login are unsupported"))
    }
  }
  Ok()
}

## Select the maintenance notice. A privileged login ignores caller overrides.
export proc nologin_file() [env, process] -> Path {
  if applet.current_euid() == 0 { p"/etc/nologin" } else { fp"{env.get_or("XSH_NOLOGIN_FILE", "/etc/nologin") ?? "/etc/nologin"}" }
}

## Collect supplemental groups before dropping credentials. Fixture data can
## select groups only for a process that already owns the fixture UID/GID.
export proc login_groups(entry: auth.PasswdEntry) [fs, env, process, error] -> Result[List[Int], Error] {
  if applet.current_euid() == 0 { return user.groups(entry.name, primary_gid: entry.gid) }
  if let Ok(configured) = e"XSH_GROUP_FILE" {
    var groups = [entry.gid]
    for group_entry in parse_groups(fp"{configured}".read_text()?) {
      if entry.name in group_entry.members and group_entry.gid not in groups { groups += [group_entry.gid] }
    }
    return Ok(groups)
  }
  user.groups(entry.name, primary_gid: entry.gid)
}

## Create the exact session environment. Preserved variables cannot replace
## identity fields, executable lookup, or shell/loader initialization controls.
export proc session_environment(entry: auth.PasswdEntry, preserve: Bool, remote: Str) [env, error] -> Result[Map[Str, Str], Error] {
  var values: Map[Str, Str] = {}
  if preserve {
    for item in env.list()? {
      if item.name.starts_with("LD_") or item.name.starts_with("DYLD_") or item.name.starts_with("XSH_") or item.name in ["ENV", "BASH_ENV", "SHELLOPTS", "BASHOPTS", "IFS"] { continue }
      values[item.name] = item.value
    }
  }
  values["HOME"] = entry.home.display()
  values["SHELL"] = entry.shell
  values["USER"] = entry.name
  values["LOGNAME"] = entry.name
  values["PATH"] = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
  if let Ok(terminal) = e"TERM" { values["TERM"] = terminal }
  if remote != "" { values["REMOTEHOST"] = remote }
  Ok(values)
}
