##! The one owner of the resolver file for DHCP clients. Each interface leaves a
##! small contribution file of nameservers and search names under the run
##! directory; every change to a contribution, and every removal, recomposes the
##! resolver file from the operator's static file followed by the contributions
##! in interface-name order, under one lock, and publishes it atomically. The
##! published file carries a marker first line. A destination without that
##! marker (a foreign file, including one a VPN manages), a symlink or any
##! other kind of entry is foreign: it is reported and never edited, replaced or
##! restored, and the contributions stay on disk for when it is released.
use dhcp_machine as machine
use dhcp_wire as wire
use nettools

## The first line of every resolver file this library publishes.
export const MARKER = "# Managed by dhcpc: edit the static file, not this one."
## The most nameservers the resolver file lists.
export const MAX_SERVERS = 3
## The most search names the resolver file lists.
export const MAX_SEARCH = 6
## The longest search list in bytes, the limit resolver libraries apply.
export const MAX_SEARCH_BYTES = 256
## The largest contribution or static file read, in bytes.
export const MAX_FILE_BYTES = 4096
## The most contribution files one directory may hold.
export const MAX_CONTRIBUTIONS = 32
## The default wait for the update lock in milliseconds.
export const LOCK_WAIT_MS = 5000

const CONTRIBUTION_VERSION = 1
const MAX_CONTRIBUTION_BYTES = 1024
const LOCK_POLL_MS = 20
const SUFFIX = ".dns"

## What one interface asks the resolver file to carry.
export type Contribution = {interface: Str, servers: List[Str], search: List[Str]}

## Where everything lives. `run_dir` holds the lock and the contributions in a
## `dns` directory, `destination_dir` holds the resolver file named
## `destination`, `static_path`, when set, is the operator's static file, and
## `lock_wait_ms` bounds the wait for the update lock.
export type Layout = {run_dir: Path, destination_dir: Path, destination: Str, static_path: Path?, lock_wait_ms: Int}

## What an update left published.
export enum Standing {
  Published,
  Unchanged,
  Conflict(Str),
}

## Why an update could not run.
export error DnsReject {
  Busy
  Invalid(message: Str)
}

type ContributionJson = {version: Int, servers: List[Str], search: List[Str]}

## The directives a static resolver file contributes.
export type Directives = {servers: List[Str], search: List[Str], options: List[Str]}

type Destination = {kind: Str, ours: Bool}

pure valid_server(text: Str) -> Bool {
  return true when wire.ip_parse(text) != null
  ":" in text and text.byte_len() <= 45 and nettools.ipv6_parse(text) != null
}

pure sorted_unique(items: List[Str]) -> List[Str] {
  var kept = []
  for item in items {
    if item not in kept {
      kept += [item]
    }
  }
  kept
}

## The contribution a lease asks for: its nameservers and its domain as the
## search name.
export pure contribution_for(interface: Str, lease: machine.Lease) -> Contribution {
  let servers = [wire.ip_text(address) for address in lease.dns] |> take(MAX_SERVERS)
  let search = if let domain = lease.domain { [domain] } else { [] }
  {interface: interface, servers: servers, search: search}
}

pure within_search_limits(search: List[Str]) -> Bool {
  return false when search.len() > MAX_SEARCH
  let used = [name.byte_len() + 1 for name in search] |> sum()
  used <= MAX_SEARCH_BYTES
}

## Checks a contribution: a safe interface name, at most three nameservers
## that parse as addresses and at most six search names that are plain domain
## names inside the search-list byte limit.
export pure contribution_ok(item: Contribution) -> Bool {
  return false unless valid_interface(item.interface)
  return false when item.servers.len() > MAX_SERVERS
  for server in item.servers {
    return false unless valid_server(server)
  }
  for name in item.search {
    return false unless wire.is_domain_name(name)
  }
  within_search_limits(item.search)
}

pure valid_interface(name: Str) -> Bool {
  return false when name == "" or name == "." or name == ".." or name.byte_len() > 15
  for character in name {
    return false when character not in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789._-"
  }
  true
}

## The JSON text of a contribution file.
export proc encode_contribution(item: Contribution) -> Result[Str, Error] {
  json.encode(ContributionJson(version: CONTRIBUTION_VERSION, servers: item.servers, search: item.search))
}

## Reads a contribution file strictly: bounded size, version 1, no unknown
## field, and values `contribution_ok` accepts.
export proc decode_contribution(interface: Str, text: Str) -> Result[Contribution, DnsReject] {
  fail .Invalid("contribution too large") when text.byte_len() > MAX_CONTRIBUTION_BYTES
  let parsed = json.decode(text) ?? { |_| fail .Invalid("not json") }
  let item = parsed.require(ContributionJson) ?? { |_| fail .Invalid("schema") }
  fail .Invalid("version") when item.version != CONTRIBUTION_VERSION
  let rebuilt = json.encode(ContributionJson(...item)) ?? { |_| fail .Invalid("schema") }
  let received = json.encode(parsed) ?? { |_| fail .Invalid("schema") }
  fail .Invalid("unknown field") when rebuilt != received
  let found = Contribution(interface:, servers: item.servers, search: item.search)
  fail .Invalid("value") unless contribution_ok(found)
  Ok(found)
}

## The directives of a static resolver file that the composer keeps:
## `nameserver` with an address, `search` or `domain` with plain domain names,
## and `options` with a conservative character set. Comments, blank lines and
## every other line are dropped.
export pure parse_static(text: Str) -> Directives {
  var servers = []
  var search = []
  var options = []
  for line in text.lines() {
    let fields = line.words()
    continue when fields.is_empty()
    match fields[0] {
      "nameserver" => {
        if fields.len() == 2 and valid_server(fields[1]) {
          servers += [fields[1]]
        }
      }
      "search" | "domain" => {
        for name in fields |> drop(1) {
          if wire.is_domain_name(name) {
            search += [name]
          }
        }
      }
      "options" => {
        for option in fields |> drop(1) {
          if plain_option(option) {
            options += [option]
          }
        }
      }
      else => {}
    }
  }
  {servers: sorted_unique(servers), search: sorted_unique(search), options: sorted_unique(options)}
}

pure plain_option(option: Str) -> Bool {
  return false when option.byte_len() > 32
  for character in option {
    return false when character not in "abcdefghijklmnopqrstuvwxyz0123456789:-_"
  }
  true
}

pure bounded_search(names: List[Str]) -> List[Str] {
  var kept = []
  var used = 0
  for name in names {
    break when kept.len() >= MAX_SEARCH or used + name.byte_len() + 1 > MAX_SEARCH_BYTES
    kept += [name]
    used += name.byte_len() + 1
  }
  kept
}

## Composes the resolver file: the marker, the static options, the static
## nameservers then those of each contribution in interface-name order, at most
## three in all, and one search line of the static names then the contributed
## ones, within the resolver limits. A file with nothing to say is the marker
## alone.
export pure compose(static: Directives, contributions: List[Contribution]) -> Str {
  let ordered = contributions |> sort-by .interface
  let servers = sorted_unique(static.servers.extend(flatten_servers(ordered))) |> take(MAX_SERVERS)
  let search = static.search.extend(flatten_search(ordered)) |> sorted_unique(_) |> bounded_search(_)
  var lines = [MARKER]
  if ! static.options.is_empty() {
    lines += [f"options {static.options.join(" ")}"]
  }
  for server in servers {
    lines += [f"nameserver {server}"]
  }
  if ! search.is_empty() {
    lines += [f"search {search.join(" ")}"]
  }
  f"{lines.join("\n")}\n"
}

pure flatten_servers(items: List[Contribution]) -> List[Str] {
  collect {
    for item in items {
      for server in item.servers {
        yield server
      }
    }
  }
}

pure flatten_search(items: List[Contribution]) -> List[Str] {
  collect {
    for item in items {
      for name in item.search {
        yield name
      }
    }
  }
}

pure contribution_name(interface: Str) -> Result[RelPath, DnsReject] {
  fail .Invalid("interface name") unless valid_interface(interface)
  if let Ok(relative) = fp"{interface}{SUFFIX}".require(RelPath) {
    Ok(relative)
  } else {
    fail .Invalid("interface name")
  }
}

proc dns_root(layout: Layout) [fs, error] -> Result[FsRoot, Error] {
  let directory = fp"{layout.run_dir}/dns"
  if ! directory.exists() {
    directory.mkdir()
  }
  fs.open_root(directory)
}

proc acquire(directory: Path, wait_ms: Int) [fs, time, error] -> Result[FsLock, Error] {
  var waited = 0
  while true {
    match fs.lock(fp"{directory}/dns.lock", nonblocking: true) {
      Ok(held) => return Ok(held)
      Err(failure) => {
        if (failure.errno ?? 0) not in [11, 35] or waited >= wait_ms {
          return Err(DnsReject.Busy("the resolver lock is held"))
        }
        time.sleep(LOCK_POLL_MS * 1ms)
        waited += LOCK_POLL_MS
      }
    }
  }
  Err(DnsReject.Busy("unreachable"))
}

proc read_contributions(root: FsRoot) [fs, error] -> Result[List[Contribution], Error] {
  let listing = root.children(p".", max_entries: MAX_CONTRIBUTIONS + 1)?
  fail "too many contribution files" when listing.children.len() > MAX_CONTRIBUTIONS
  collect {
    for child in listing.children {
      let name = child.display()
      continue unless name.ends_with(SUFFIX)
      let interface = name.byte_slice(0, length: name.byte_len() - SUFFIX.byte_len())
      continue unless valid_interface(interface)
      let snapshot = root.read_result(child, max_bytes: MAX_CONTRIBUTION_BYTES + 1)?
      continue unless snapshot.state == "observed" and ! snapshot.truncated
      let text = (snapshot.data ?? b"").utf8() ?? ""
      if let Ok(item) = decode_contribution(interface, text) {
        yield item
      }
    }
  }
}

proc read_static(layout: Layout) [fs, error] -> Result[Directives, Error] {
  guard let source = layout.static_path else { return Ok(Directives(servers: [], search: [], options: [])) }
  let parent = source.parent()
  guard parent.exists() else { return Ok(Directives(servers: [], search: [], options: [])) }
  let root = fs.open_root(parent)?
  let snapshot = root.read_result(fp"{source.name()}".require()?, max_bytes: MAX_FILE_BYTES + 1)?
  guard snapshot.state == "observed" and ! snapshot.truncated else { return Ok(Directives(servers: [], search: [], options: [])) }
  Ok(parse_static((snapshot.data ?? b"").utf8() ?? ""))
}

proc inspect(root: FsRoot, name: RelPath) [fs, error] -> Result[Destination, Error] {
  let observed = root.stat(name)
  if let Err(failure) = observed {
    return Ok({kind: "absent", ours: false}) when failure.errno == 2
    return Err(failure)
  }
  let stat = observed?
  guard stat.kind == "file" else { return Ok({kind: stat.kind, ours: false}) }
  let snapshot = root.read_result(name, max_bytes: MAX_FILE_BYTES)?
  let head = (snapshot.data ?? b"").utf8() ?? ""
  Ok({kind: "file", ours: head.starts_with(f"{MARKER}\n")})
}

pure conflict_reason(destination: Destination) -> Str {
  if destination.kind == "symlink" {
    "symlink"
  } else if destination.kind == "file" {
    "foreign_file"
  } else {
    "special_entry"
  }
}

## Sets (or, for null, removes) the contribution of `interface` and recomposes
## the resolver file under the update lock. A foreign destination is reported
## as `Conflict` and left untouched; the contribution is still recorded. Only
## the named interface's file is ever removed.
export proc update(layout: Layout, interface: Str, contribution: Contribution?) [fs, time, error] -> Result[Standing, Error] {
  let file = contribution_name(interface)?
  if let item = contribution {
    if item.interface != interface or ! contribution_ok(item) {
      return Err(DnsReject.Invalid("contribution"))
    }
  }
  let root = dns_root(layout)?
  let lock = acquire(fp"{layout.run_dir}/dns", layout.lock_wait_ms)?
  defer fs.unlock(lock)
  if let item = contribution {
    root.write_atomic(file, encode_contribution(item)?, mode: 0o644, replace: "regular_or_absent")
  } else if root.exists(file) {
    root.remove(file)
  }
  let text = compose(read_static(layout)?, read_contributions(root)?)
  let destination_root = fs.open_root(layout.destination_dir)?
  let destination = fp"{layout.destination}".require(RelPath)?
  let standing = inspect(destination_root, destination)?
  if standing.kind != "absent" and ! standing.ours {
    return Ok(Conflict(conflict_reason(standing)))
  }
  if standing.ours and destination_root.read_text(destination)? == text {
    return Ok(Unchanged)
  }
  destination_root.write_atomic(destination, text, mode: 0o644, replace: "regular_or_absent")
  Ok(Published)
}
