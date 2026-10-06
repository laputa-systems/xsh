#!/bin/xsh
use lib.gnu
use lib.accounts

type Options = {service: Str?, help: Bool, version: Bool, arguments: List[Str]}

proc main(...argv: List[Str]) [fs, env, process, io, error, net] {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 1},
    service: {form: "-s --service SERVICE"},
    help: {form: "--help", default: false, stop: true},
    version: {form: "-V --version", default: false, stop: true},
    arguments: {form: "...ARG"},
  })?
  if opts.help { gnu.help("Usage: getent [-s files|dns] DATABASE [KEY]...\nRead passwd, group, hosts, services, or protocols records.\nThe files backend supports enumeration. The dns backend supports keyed hosts lookups."); return }
  if opts.version { gnu.version("getent"); return }
  if opts.arguments.is_empty() { gnu.missing_operand() }
  let database = opts.arguments[0]
  if database not in ["passwd", "group", "hosts", "services", "protocols"] { gnu.error(f"unknown database: {database}"); exit 1 }
  let service = opts.service ?? "files"
  if service != "files" and service != "dns" { gnu.error(f"unsupported service: {service}"); exit 1 }
  if service == "dns" and database != "hosts" { gnu.error("the dns service supports only hosts"); exit 1 }
  let keys = opts.arguments |> drop(1)
  if service == "dns" {
    if keys.is_empty() { gnu.error("enumeration is not supported by the dns service"); exit 3 }
    var missing = false
    for key in keys {
      let result = accounts.resolve_hosts(key)
      if let Ok(entries) = result {
        if entries.is_empty() { missing = true }
        for item in entries { gnu.write_text(item + "\n") }
      } else if let Err(failure) = result { gnu.error(failure.message); missing = true }
    }
    if missing { exit 2 }
    return
  }
  let source = accounts.database_file(database)?
  let content = source.read_text()
  if let Err(failure) = content { gnu.name_error(source.display(), failure); exit 1 }
  let rows = accounts.lookup_rows(database, content?)?
  if keys.is_empty() { for item in rows { gnu.write_text(item.text + "\n") }; return }
  var missing = false
  for key in keys {
    var sought = if database == "hosts" { key.lower() } else { key }
    if database in ["passwd", "group", "protocols"] and rx"^[0-9]+$".matches(key) {
      if let Ok(number) = key.parse_int() { sought = f"{number}" }
    }
    var found = false
    for item in rows {
      if sought in item.keys { gnu.write_text(item.text + "\n"); found = true; if database != "hosts" { break } }
    }
    if !found and database == "hosts" and opts.service == null and e"XSH_HOSTS_FILE" is Err(_) {
      let resolved = accounts.resolve_hosts(key)
      if let Ok(entries) = resolved {
        for item in entries { gnu.write_text(item + "\n"); found = true }
      } else if let Err(failure) = resolved { gnu.error(failure.message) }
    }
    if !found { missing = true }
  }
  if missing { exit 2 }
}
