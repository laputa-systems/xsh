##! The persisted lease record of one interface: a versioned JSON document that
##! holds the lease the client last held and the addresses and routes it owns.
##! A record is a claim, never a fact. Every field is read through a strict
##! schema, and a remembered lease is revalidated against the current boot,
##! interface index and hardware address before it seeds anything; even then it
##! only names an address to ask the server about again.
use dhcp_machine as machine
use dhcp_net as network
use dhcp_wire as wire

## The version of the document layout this library reads and writes.
export const RECORD_VERSION = 2
## The largest record the loader reads, in bytes.
export const MAX_RECORD_BYTES = 65536

const MAX_NAME_BYTES = 15
const MAX_ROUTES = 8
const MAX_CLASSLESS = wire.MAX_CLASSLESS_ROUTES
const MAX_DNS = 8
const MAX_DOMAIN_BYTES = 253

## Everything a restart needs to recognize its own state.
export type LeaseRecord = {
  name: Str,
  ifindex: Int,
  mac: Bytes,
  client_id: Bytes,
  boot_id: Str,
  boottime_ms: Int,
  lease: machine.Lease?,
  owned: network.Owned,
  reconcile: Bool,
}

## Why a document was refused.
export error StoreReject {
  TooLarge(limit: Int)
  NotJson(message: Str)
  Version(found: Int)
  Schema(message: Str)
  Unknown
  Value(message: Str)
}

type ClasslessJson = {destination: Str, prefix: Int, gateway: Str}

type LeaseJson = {
  address: Str,
  prefix: Int,
  broadcast: Str,
  routers: List[Str],
  classless_routes: List[ClasslessJson],
  dns: List[Str],
  domain: Str?,
  server: Str,
  lease_secs: Int,
  acquired_at: Int,
  renew_at: Int?,
  rebind_at: Int?,
  expire_at: Int?,
  timers: Str,
}

type AddressJson = {address: Str, prefix: Int, broadcast: Str}

type RouteJson = {destination: Str, prefix: Int, gateway: Str, table: Int, tos: Int, priority: Int, protocol: Int}

type OwnedJson = {ifindex: Int, addresses: List[AddressJson], routes: List[RouteJson]}

type RecordJson = {
  version: Int,
  name: Str,
  ifindex: Int,
  mac: Str,
  client_id: Str,
  boot_id: Str,
  boottime_ms: Int,
  lease: LeaseJson?,
  owned: OwnedJson,
  reconcile: Bool,
}

pure hex_of(data: Bytes) -> Str {
  var text = ""
  for index in range(data.len()) {
    let value = data.byte_at(index) ?? 0
    text += "0123456789abcdef".byte_slice(value / 16, length: 1) + "0123456789abcdef".byte_slice(value % 16, length: 1)
  }
  text
}

pure hex_to_bytes(text: Str) -> Bytes? {
  return null when text.byte_len() % 2 != 0 or text.byte_len() > 512
  let values = collect {
    var index = 0
    while index < text.byte_len() {
      let high = "0123456789abcdef".find(text.byte_slice(index, length: 1))
      let low = "0123456789abcdef".find(text.byte_slice(index + 1, length: 1))
      return null when high == null or low == null
      yield high * 16 + low
      index += 2
    }
  }
  if let Ok(packed) = bytes.from_ints(values) {
    packed
  } else {
    null
  }
}

pure ips_of(items: List[Int]) -> List[Str] {
  [wire.ip_text(item) for item in items]
}

pure lease_json(lease: machine.Lease) -> LeaseJson {
  LeaseJson(
    address: wire.ip_text(lease.address),
    prefix: lease.prefix,
    broadcast: wire.ip_text(lease.broadcast),
    routers: ips_of(lease.routers),
    classless_routes: [ClasslessJson(destination: wire.ip_text(item.destination), prefix: item.prefix, gateway: wire.ip_text(item.gateway)) for item in lease.classless],
    dns: ips_of(lease.dns),
    domain: lease.domain,
    server: wire.ip_text(lease.server),
    lease_secs: lease.lease_secs,
    acquired_at: lease.acquired_at,
    renew_at: lease.renew_at,
    rebind_at: lease.rebind_at,
    expire_at: lease.expire_at,
    timers: timer_name(lease.timers),
  )
}

pure timer_name(rule: machine.TimerRule) -> Str {
  match rule {
    machine.Infinite => "infinite"
    machine.Server => "server"
    machine.Mixed => "mixed"
    machine.Default => "default"
    machine.Repaired => "repaired"
  }
}

pure timer_rule(name: Str) -> machine.TimerRule? {
  match name {
    "infinite" => machine.Infinite
    "server" => machine.Server
    "mixed" => machine.Mixed
    "default" => machine.Default
    "repaired" => machine.Repaired
    else => null
  }
}

pure owned_json(owned: network.Owned) -> OwnedJson {
  OwnedJson(
    ifindex: owned.ifindex,
    addresses: [AddressJson(address: wire.ip_text(item.address), prefix: item.prefix, broadcast: wire.ip_text(item.broadcast)) for item in owned.addresses],
    routes: [
      RouteJson(
        destination: wire.ip_text(item.destination),
        prefix: item.prefix,
        gateway: wire.ip_text(item.gateway),
        table: item.table,
        tos: item.tos,
        priority: item.priority,
        protocol: item.protocol,
      )
      for item in owned.routes
    ],
  )
}

pure document(record: LeaseRecord) -> RecordJson {
  RecordJson(
    version: RECORD_VERSION,
    name: record.name,
    ifindex: record.ifindex,
    mac: hex_of(record.mac),
    client_id: hex_of(record.client_id),
    boot_id: record.boot_id,
    boottime_ms: record.boottime_ms,
    lease: if let lease = record.lease { lease_json(lease) } else { null },
    owned: owned_json(record.owned),
    reconcile: record.reconcile,
  )
}

## The JSON text of a record.
export proc encode(record: LeaseRecord) -> Result[Str, Error] {
  json.encode(document(record))
}

pure address_of(text: Str, what: Str) -> Result[Int, StoreReject] {
  guard let value = wire.ip_parse(text) else { fail .Value(what) }
  value
}

pure addresses_of(items: List[Str], what: Str, limit: Int) -> Result[List[Int], StoreReject] {
  fail .Value(what) when items.len() > limit
  [address_of(item, what)? for item in items]
}

pure prefix_of(prefix: Int, what: Str) -> Result[Int, StoreReject] {
  fail .Value(what) when prefix < 0 or prefix > 32
  prefix
}

pure classless_of(items: List[ClasslessJson]) -> Result[List[wire.ClasslessRoute], StoreReject] {
  fail .Value("classless routes") when items.len() > MAX_CLASSLESS
  var routes = []
  for entry in items {
    let prefix = prefix_of(entry.prefix, "classless prefix")?
    let destination = address_of(entry.destination, "classless destination")?
    fail .Value("classless destination") unless destination.bit_and(wire.prefix_mask(prefix) ?? 0) == destination
    let gateway = address_of(entry.gateway, "classless gateway")?
    fail .Value("classless gateway") unless gateway == 0 or wire.is_unicast(gateway)
    routes += [wire.ClasslessRoute(destination:, prefix:, gateway:)]
  }
  Ok(routes)
}

pure lease_of(item: LeaseJson) -> Result[machine.Lease, StoreReject] {
  guard let rule = timer_rule(item.timers) else { fail .Value("timers") }
  fail .Value("lease_secs") when item.lease_secs < 0 or item.lease_secs > wire.INFINITE_LEASE
  fail .Value("domain") when (item.domain ?? "").byte_len() > MAX_DOMAIN_BYTES
  Ok(
    machine.Lease(
      address: address_of(item.address, "address")?,
      prefix: prefix_of(item.prefix, "prefix")?,
      broadcast: address_of(item.broadcast, "broadcast")?,
      routers: addresses_of(item.routers, "routers", MAX_ROUTES)?,
      classless: classless_of(item.classless_routes)?,
      dns: addresses_of(item.dns, "dns", MAX_DNS)?,
      domain: item.domain,
      server: address_of(item.server, "server")?,
      lease_secs: item.lease_secs,
      acquired_at: item.acquired_at,
      renew_at: item.renew_at,
      rebind_at: item.rebind_at,
      expire_at: item.expire_at,
      timers: rule,
    ),
  )
}

pure owned_of(item: OwnedJson) -> Result[network.Owned, StoreReject] {
  fail .Value("owned") when item.addresses.len() > network.MAX_OWNED or item.routes.len() > network.MAX_OWNED
  let addresses = [
    network.Address(address: address_of(entry.address, "owned address")?, prefix: prefix_of(entry.prefix, "owned prefix")?, broadcast: address_of(entry.broadcast, "owned broadcast")?)
    for entry in item.addresses
  ]
  var routes = []
  for entry in item.routes {
    fail .Value("owned route") when entry.table < 0 or entry.table > 255 or entry.tos < 0 or entry.tos > 255 or entry.protocol < 0 or entry.protocol > 255 or entry.priority < 0
    routes += [
      network.Route(
        destination: address_of(entry.destination, "owned destination")?,
        prefix: prefix_of(entry.prefix, "owned route prefix")?,
        gateway: address_of(entry.gateway, "owned gateway")?,
        table: entry.table,
        tos: entry.tos,
        priority: entry.priority,
        protocol: entry.protocol,
      ),
    ]
  }
  Ok({ifindex: item.ifindex, addresses: addresses, routes: routes})
}

## True for a name that is safe as an interface name and as a file name.
export pure interface_name_ok(name: Str) -> Bool {
  return false when name == "" or name == "." or name == ".." or name.byte_len() > MAX_NAME_BYTES
  for character in name {
    return false when character not in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789._-"
  }
  true
}

## Reads a record document strictly: bounded size, a known version, every
## field of the schema and no field outside it, addresses that parse as
## dotted quads, and bounded lists. Anything else is a typed rejection.
export proc decode(text: Str) -> Result[LeaseRecord, StoreReject] {
  fail .TooLarge(MAX_RECORD_BYTES) when text.byte_len() > MAX_RECORD_BYTES
  let parsed = json.decode(text) ?? { |failure| fail .NotJson(failure.message) }
  let version = json.get(parsed, ["version"]) ?? { |_| fail .Schema("version") }
  let found = version.require(Int) ?? { |_| fail .Schema("version") }
  fail .Version(found) when found != RECORD_VERSION
  let item = parsed.require(RecordJson) ?? { |failure| fail .Schema(failure.message) }
  fail .Value("name") unless interface_name_ok(item.name)
  guard let mac = hex_to_bytes(item.mac) else { fail .Value("mac") }
  guard let client_id = hex_to_bytes(item.client_id) else { fail .Value("client_id") }
  fail .Value("mac") when mac.len() != 6
  fail .Value("boot_id") when item.boot_id.byte_len() > 64
  fail .Value("owned") when item.owned.ifindex != item.ifindex
  let lease = if let held = item.lease { lease_of(held)? } else { null }
  let record = LeaseRecord(
    name: item.name,
    ifindex: item.ifindex,
    mac:,
    client_id:,
    boot_id: item.boot_id,
    boottime_ms: item.boottime_ms,
    lease:,
    owned: owned_of(item.owned)?,
    reconcile: item.reconcile,
  )
  let rebuilt = json.encode(document(record)) ?? { |failure| fail .Schema(failure.message) }
  let received = json.encode(parsed) ?? { |failure| fail .Schema(failure.message) }
  fail .Unknown() when rebuilt != received
  Ok(record)
}

## What a restart knows about the interface it finds.
export type Current = {name: Str, ifindex: Int, mac: Bytes, boot_id: Str}

## A remembered record after revalidation. `lease` may only seed an INIT-REBOOT
## request; its times mean something only when `same_boot` holds. `owned` is
## what may still exist on this interface.
export type Revived = {lease: machine.Lease?, owned: network.Owned, same_boot: Bool, same_link: Bool, reconcile: Bool, note: Str}

## Revalidates a remembered record against the interface now present. A record
## for another name or hardware address is not this interface's, so it yields
## nothing. A changed interface index means the device was recreated and what
## it owned went with it. A different boot discards the owned state and the
## clock assumptions of the lease.
export pure revalidate(record: LeaseRecord?, current: Current) -> Revived {
  let nothing = Revived(lease: null, owned: network.empty_owned(current.ifindex), same_boot: false, same_link: false, reconcile: false, note: "")
  guard let held = record else { return nothing }
  return {...nothing, note: "different_interface"} when held.name != current.name or held.mac != current.mac
  let same_boot = held.boot_id == current.boot_id and held.boot_id != ""
  let same_link = held.ifindex == current.ifindex
  let owned = if same_boot and same_link { held.owned } else { network.empty_owned(current.ifindex) }
  let note = if ! same_boot { "different_boot" } else if ! same_link { "interface_recreated" } else { "" }
  Revived(lease: held.lease, owned:, same_boot:, same_link:, reconcile: held.reconcile, note:)
}

## True when the interface carries every address the revived record owns,
## which is what makes a remembered lease count as configured.
export pure configured(revived: Revived, observed: network.Observation) -> Bool {
  return false unless revived.same_boot and revived.same_link
  guard let lease = revived.lease else { return false }
  for item in revived.owned.addresses {
    return true when item.address == lease.address and network.holds_address(observed.addresses, item)
  }
  false
}

## The record path for an interface below a state directory root.
export pure record_path(name: Str) -> Result[RelPath, StoreReject] {
  fail .Value("name") unless interface_name_ok(name)
  if let Ok(relative) = fp"{name}.lease".require(RelPath) {
    Ok(relative)
  } else {
    fail .Value("name")
  }
}

## Reads the record of `name` below `root`. A missing file is no record; a
## file over the size limit, unreadable or malformed is an error, never an
## empty record.
export proc load(root: FsRoot, name: Str) [fs, error] -> Result[LeaseRecord?, StoreReject] {
  let relative = record_path(name)?
  let snapshot = root.read_result(relative, max_bytes: MAX_RECORD_BYTES + 1) ?? { |failure| fail .NotJson(failure.message) }
  return null when snapshot.state == "absent"
  fail .NotJson(snapshot.state) unless snapshot.state == "observed"
  fail .TooLarge(MAX_RECORD_BYTES) when snapshot.truncated
  let data = snapshot.data ?? b""
  let text = data.utf8() ?? { |failure| fail .NotJson(failure.message) }
  decode(text)?
}

## Writes the record of its interface below `root` atomically with mode 0600,
## refusing to replace a symlink, directory or special file.
export proc save(root: FsRoot, record: LeaseRecord) [fs, error] -> Result[Unit, Error] {
  let relative = record_path(record.name)?
  root.write_atomic(relative, encode(record)?, mode: 0o600, replace: "regular_or_absent")
}
