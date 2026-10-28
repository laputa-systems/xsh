##! Ownership-aware network configuration for a DHCP lease. The client owns
##! exactly the addresses and routes it recorded; everything else on the
##! interface is foreign and is never changed. A plan is computed from a
##! complete observation of one link, applied operation by operation with the
##! intent persisted first, and checked against a fresh observation afterwards.
##! Nothing here flushes an interface, and an interface name alone never
##! confers ownership: ownership is an exact (index, address, prefix) or route
##! key in the owned record.
use dhcp_machine as machine
use dhcp_wire as wire

## One IPv4 address on a link, with the broadcast address it is added with.
export type Address = {address: Int, prefix: Int, broadcast: Int}

## One IPv4 route. Its identity is every field: the kernel keys on the
## destination, table, tos and priority, and a delete is restricted by the
## gateway and protocol as well.
export type Route = {destination: Int, prefix: Int, gateway: Int, table: Int, tos: Int, priority: Int, protocol: Int}

## What the client created on one interface index. A record for another index
## owns nothing here.
export type Owned = {ifindex: Int, addresses: List[Address], routes: List[Route]}

## One link with the IPv4 state attached to it, as a complete observation
## saw it. Routes are those whose single next hop leaves through the link.
export type Observation = {
  ifindex: Int,
  name: Str,
  mac: Bytes,
  up: Bool,
  carrier: Bool,
  addresses: List[Address],
  routes: List[Route],
}

## One change the client may make.
export enum Operation {
  BringUp,
  AddAddress(Address),
  RemoveAddress(Address),
  AddRoute(Route),
  RemoveRoute(Route),
}

## Something foreign that stands where the client would put its own state.
## It is reported and left untouched.
export type Conflict = {kind: Str, subject: Str}

## What to do, why nothing else is done, and what the owned record will be if
## every operation succeeds. `before` is the observation the plan was made
## from, which an applier compares with a fresh one before it changes anything.
export type Plan = {operations: List[Operation], conflicts: List[Conflict], owned_after: Owned, before: Observation}

## How a reconciliation ended.
export enum Settled {
  Converged,
  Conflicted,
  Partial(Str),
  Stale,
}

## The result of one reconciliation.
export type Outcome = {settled: Settled, owned: Owned, conflicts: List[Conflict]}

## Why an observation cannot be trusted.
export error ObserveReject {
  Incomplete(state: Str)
  NoSuchLink(name: Str)
  AmbiguousLink(name: Str)
  BadValue(detail: Str)
}

## The main routing table.
export const TABLE_MAIN = 254
## The routing protocol number the client tags its routes with.
export const PROTOCOL_DHCP = 16
## The priority of the default route the client installs unless told otherwise.
export const DEFAULT_METRIC = 100
## The most owned addresses or routes one record may hold.
export const MAX_OWNED = 16
## The most reconciliation rounds before the outcome is reported partial.
export const MAX_ROUNDS = 3

const KERNEL_PROTOCOL = 2

## A request that changes one object over route netlink.
export type Request = {message_type: Int, flags: Int, payload: Bytes}

## The empty owned record for an interface index.
export pure empty_owned(ifindex: Int) -> Owned {
  {ifindex: ifindex, addresses: [], routes: []}
}

pure text_address(text: Str?) -> Int? {
  guard let value = text else { return null }
  wire.ip_parse(value)
}

pure same_address(left: Address, right: Address) -> Bool {
  left.address == right.address and left.prefix == right.prefix
}

## True when `items` holds the address with the same prefix.
export pure holds_address(items: List[Address], wanted: Address) -> Bool {
  for item in items {
    return true when same_address(item, wanted)
  }
  false
}

## True when `items` holds exactly the route `wanted`.
export pure holds_route(items: List[Route], wanted: Route) -> Bool {
  for item in items {
    return true when item == wanted
  }
  false
}

pure same_route_slot(left: Route, right: Route) -> Bool {
  left.destination == right.destination and left.prefix == right.prefix and left.table == right.table and left.tos == right.tos and left.priority == right.priority
}

pure link_name(link: LinuxNetworkLink) -> Str {
  link.name ?? ""
}

pure address_fact(entry: LinuxNetworkAddress) -> Result[Address, ObserveReject] {
  let primary = text_address(entry.local)
  let found = if primary == null { text_address(entry.address) } else { primary }
  guard let local = found else { fail .BadValue("address") }
  let broadcast = text_address(entry.broadcast) ?? 0
  Ok({address: local, prefix: entry.prefix_length, broadcast: broadcast})
}

pure route_fact(entry: LinuxNetworkRoute) -> Result[Route, ObserveReject] {
  let destination = if entry.destination == null { 0 } else { text_address(entry.destination) ?? -1 }
  let gateway = if entry.gateway == null { 0 } else { text_address(entry.gateway) ?? -1 }
  fail .BadValue("route") when destination < 0 or gateway < 0
  Ok({destination: destination, prefix: entry.destination_prefix_length, gateway: gateway, table: entry.table, tos: entry.tos, priority: entry.priority ?? 0, protocol: entry.protocol})
}

pure leaves_through(entry: LinuxNetworkRoute, ifindex: Int) -> Bool {
  return entry.output_ifindex == ifindex when entry.nexthops.is_empty()
  entry.nexthops.len() == 1 and entry.nexthops[0].ifindex == ifindex
}

## The observation of the link `name` in a network dump. A dump that was
## interrupted, truncated or reported any issue is refused rather than read as
## an empty interface, as is a name that matches no link or more than one.
export pure observe_link(dump: LinuxNetworkDump, name: Str) -> Result[Observation, ObserveReject] {
  fail .Incomplete(dump.state) unless dump.enumeration_succeeded and dump.state == "complete" and dump.issues.is_empty()
  let matches = [link for link in dump.links if link_name(link) == name]
  fail .NoSuchLink(name) when matches.is_empty()
  fail .AmbiguousLink(name) when matches.len() > 1
  let link = matches[0]
  let c = linux.net_constants()
  let addresses = [entry for entry in dump.addresses if entry.family == "inet" and entry.ifindex == link.ifindex]
  let routes = [entry for entry in dump.routes if entry.family == "inet" and leaves_through(entry, link.ifindex)]
  let facts = [address_fact(entry)? for entry in addresses]
  let paths = [route_fact(entry)? for entry in routes]
  Ok(
    Observation(
      ifindex: link.ifindex,
      name:,
      mac: link.address ?? b"",
      up: link.flags.bit_and(c.IFF_UP) != 0,
      carrier: link.flags.bit_and(c.IFF_LOWER_UP) != 0,
      addresses: facts,
      routes: paths,
    ),
  )
}

pure present_address(observed: Observation, wanted: Address) -> Bool {
  holds_address(observed.addresses, wanted)
}

pure same_ip_other_prefix(observed: Observation, wanted: Address) -> Bool {
  for item in observed.addresses {
    return true when item.address == wanted.address and item.prefix != wanted.prefix
  }
  false
}

pure address_key(item: Address) -> Str {
  f"{wire.ip_text(item.address)}/{item.prefix}"
}

pure route_key(item: Route) -> Str {
  f"{wire.ip_text(item.destination)}/{item.prefix} via {wire.ip_text(item.gateway)} table {item.table} tos {item.tos} metric {item.priority} proto {item.protocol}"
}

pure desired_address(lease: machine.Lease) -> Address {
  {address: lease.address, prefix: lease.prefix, broadcast: lease.broadcast}
}

## The default route a lease asks for: its first router, installed in the main
## table with the client's protocol and `metric` as priority. Further routers
## are ignored. A lease without a router asks for none.
export pure desired_routes(lease: machine.Lease, metric: Int) -> List[Route] {
  guard ! lease.routers.is_empty() else { return [] }
  [{destination: 0, prefix: 0, gateway: lease.routers[0], table: TABLE_MAIN, tos: 0, priority: metric, protocol: PROTOCOL_DHCP}]
}

pure without_address(items: List[Address], gone: Address) -> List[Address] {
  [item for item in items if ! same_address(item, gone)]
}

pure without_route(items: List[Route], gone: Route) -> List[Route] {
  [item for item in items if item != gone]
}

pure plan_removals(desired_addresses: List[Address], desired_paths: List[Route], owned: Owned, observed: Observation) -> Plan {
  let stale_routes = [held for held in owned.routes if ! holds_route(desired_paths, held)]
  let stale_addresses = [held for held in owned.addresses if ! holds_address(desired_addresses, held)]
  let operations = [RemoveRoute(held) for held in stale_routes if holds_route(observed.routes, held)]
    .extend([RemoveAddress(held) for held in stale_addresses if present_address(observed, held)])
  let remaining = {
    ifindex: owned.ifindex,
    addresses: [held for held in owned.addresses if holds_address(desired_addresses, held)],
    routes: [held for held in owned.routes if holds_route(desired_paths, held)],
  }
  {operations: operations, conflicts: [], owned_after: remaining, before: observed}
}

pure plan_additions(desired_addresses: List[Address], desired_paths: List[Route], base: Plan) -> Plan {
  let observed = base.before
  let view = {...observed, addresses: surviving_addresses(observed, base.operations)}
  var operations = base.operations
  var conflicts = base.conflicts
  var owned = base.owned_after
  for wanted in desired_addresses {
    if holds_address(owned.addresses, wanted) {
      if ! present_address(view, wanted) {
        operations += [AddAddress(wanted)]
      }
    } else if present_address(view, wanted) {
      conflicts += [{kind: "foreign_address", subject: address_key(wanted)}]
    } else if same_ip_other_prefix(view, wanted) {
      conflicts += [{kind: "address_other_prefix", subject: address_key(wanted)}]
    } else {
      operations += [AddAddress(wanted)]
      owned = {...owned, addresses: owned.addresses.push(wanted)}
    }
  }
  for wanted in desired_paths {
    if holds_route(observed.routes, wanted) {
      if ! holds_route(owned.routes, wanted) {
        conflicts += [{kind: "foreign_route", subject: route_key(wanted)}]
      }
    } else if ! slot_taken(surviving_routes(observed, base.operations), wanted) {
      operations += [AddRoute(wanted)]
      owned = {...owned, routes: owned.routes.push(wanted)}
    } else {
      conflicts += [{kind: "foreign_route", subject: route_key(wanted)}]
    }
  }
  {operations: operations, conflicts: conflicts, owned_after: owned, before: observed}
}

pure surviving_addresses(observed: Observation, operations: List[Operation]) -> List[Address] {
  var surviving = observed.addresses
  for operation in operations {
    if let RemoveAddress(gone) = operation {
      surviving = without_address(surviving, gone)
    }
  }
  surviving
}

pure surviving_routes(observed: Observation, operations: List[Operation]) -> List[Route] {
  var surviving = observed.routes
  for operation in operations {
    if let RemoveRoute(gone) = operation {
      surviving = without_route(surviving, gone)
    }
  }
  surviving
}

pure slot_taken(routes: List[Route], wanted: Route) -> Bool {
  for item in routes {
    return true when same_route_slot(item, wanted)
  }
  false
}

## The changes that bring `observed` to the lease `desired` (or, for null, to
## nothing), touching only what `owned` records. Owned state that is no longer
## wanted is removed when present and forgotten when already gone. Wanted state
## that a foreign object occupies is reported as a conflict and left alone, and
## the connected route the kernel adds with an address is not tracked: it
## leaves with the address. Routes are removed before addresses and added after
## them, because a gateway must lie inside a connected subnet.
export pure plan(desired: machine.Lease?, metric: Int, owned: Owned, observed: Observation) -> Plan {
  let current = if owned.ifindex == observed.ifindex { owned } else { empty_owned(observed.ifindex) }
  let addresses = if let lease = desired { [desired_address(lease)] } else { [] }
  let paths = if let lease = desired { desired_routes(lease, metric) } else { [] }
  let removals = plan_removals(addresses, paths, current, observed)
  let additions = plan_additions(addresses, paths, removals)
  let wake = if ! observed.up and ! addresses.is_empty() { [BringUp] } else { [] }
  {...additions, operations: wake.extend(additions.operations)}
}

## The ways a fresh observation differs from what a successful plan leaves:
## wanted objects that are missing and removed objects that remain. An empty
## list means the plan converged.
export pure verify(plan: Plan, after: Observation) -> List[Str] {
  collect {
    for operation in plan.operations {
      match operation {
        BringUp => yield "link_down" unless after.up
        AddAddress(item) => yield f"missing_address {address_key(item)}" unless present_address(after, item)
        RemoveAddress(item) => yield f"lingering_address {address_key(item)}" when present_address(after, item)
        AddRoute(item) => yield f"missing_route {route_key(item)}" unless holds_route(after.routes, item)
        RemoveRoute(item) => yield f"lingering_route {route_key(item)}" when holds_route(after.routes, item)
      }
    }
  }
}

pure attribute(kind: Int, data: Bytes) -> Result[Bytes, Error] {
  let length = 4 + data.len()
  let padding = (4 - length % 4) % 4
  Ok(bytes.concat([bytes.pack_le(length, 2)?, bytes.pack_le(kind, 2)?, data, bytes.zero(padding)?]))
}

pure be32(value: Int) -> Result[Bytes, Error] {
  bytes.pack_be(value, 4)
}

## The route-netlink request that adds or removes an address on a link. An
## addition fails with EEXIST when the address is already there; a removal
## names the exact address and prefix.
export pure address_request(item: Address, ifindex: Int, add: Bool) -> Result[Request, Error] {
  let c = linux.net_constants()
  let header = bytes.concat([bytes.from_ints([c.AF_INET, item.prefix, 0, c.RT_SCOPE_UNIVERSE])?, bytes.pack_le(ifindex, 4)?])
  let local = attribute(c.IFA_LOCAL, be32(item.address)?)?
  let peer = attribute(c.IFA_ADDRESS, be32(item.address)?)?
  let announced = if add { attribute(c.IFA_BROADCAST, be32(item.broadcast)?)? } else { b"" }
  let flags = if add { c.NLM_F_REQUEST.bit_or(c.NLM_F_ACK).bit_or(c.NLM_F_CREATE).bit_or(c.NLM_F_EXCL) } else { c.NLM_F_REQUEST.bit_or(c.NLM_F_ACK) }
  Ok({message_type: if add { c.RTM_NEWADDR } else { c.RTM_DELADDR }, flags: flags, payload: bytes.concat([header, local, peer, announced])})
}

## The route-netlink request that adds or removes a route through a link. An
## addition is exclusive; a removal names the destination, gateway, link,
## priority, protocol and tos, and leaves scope and type as wildcards.
export pure route_request(item: Route, ifindex: Int, add: Bool) -> Result[Request, Error] {
  let c = linux.net_constants()
  let scope = if add { c.RT_SCOPE_UNIVERSE } else { c.RT_SCOPE_NOWHERE }
  let kind = if add { c.RTN_UNICAST } else { 0 }
  let header = bytes.concat([bytes.from_ints([c.AF_INET, item.prefix, 0, item.tos, item.table, item.protocol, scope, kind])?, bytes.zero(4)?])
  let destination = if item.prefix > 0 { attribute(c.RTA_DST, be32(item.destination)?)? } else { b"" }
  let via = attribute(c.RTA_GATEWAY, be32(item.gateway)?)?
  let device = attribute(c.RTA_OIF, bytes.pack_le(ifindex, 4)?)?
  let priority = attribute(c.RTA_PRIORITY, bytes.pack_le(item.priority, 4)?)?
  let flags = if add { c.NLM_F_REQUEST.bit_or(c.NLM_F_ACK).bit_or(c.NLM_F_CREATE).bit_or(c.NLM_F_EXCL) } else { c.NLM_F_REQUEST.bit_or(c.NLM_F_ACK) }
  Ok({message_type: if add { c.RTM_NEWROUTE } else { c.RTM_DELROUTE }, flags: flags, payload: bytes.concat([header, destination, via, device, priority])})
}

## Where a reconciliation is waiting.
export enum Phase {
  Observing,
  Logging,
  Rechecking,
  Applying,
  Confirming,
  Saving,
  Closed,
}

## An owned record to write, flagged when the next run must reconcile it
## against observation.
export type Ledger = {owned: Owned, reconcile: Bool}

## What the caller must do next for a reconciliation.
export enum Action {
  DoObserve,
  DoPersist(Ledger),
  DoMutate(Operation),
  Finished(Outcome),
}

## What happened when the caller did it.
export enum Feedback {
  Observed(Observation),
  Persisted,
  Mutated,
  MutationFailed(Str),
}

## One reconciliation in progress. `intent` is the owned record written before
## any change, a superset of what is owned now, so a crash never leaves a
## created object unrecorded.
export type Walk = {
  phase: Phase,
  desired: machine.Lease?,
  metric: Int,
  owned: Owned,
  round: Int,
  plan: Plan?,
  pending: List[Operation],
  intent: Owned,
}

## A reconciliation after one step, with the next thing to do.
export type Move = {walk: Walk, action: Action}

pure union_addresses(left: List[Address], right: List[Address]) -> List[Address] {
  var merged = left
  for item in right {
    if ! holds_address(merged, item) {
      merged += [item]
    }
  }
  merged
}

pure union_routes(left: List[Route], right: List[Route]) -> List[Route] {
  var merged = left
  for item in right {
    if ! holds_route(merged, item) {
      merged += [item]
    }
  }
  merged
}

pure union_owned(left: Owned, right: Owned) -> Owned {
  {ifindex: right.ifindex, addresses: union_addresses(left.addresses, right.addresses), routes: union_routes(left.routes, right.routes)}
}

pure close(walk: Walk, settled: Settled, owned: Owned, conflicts: List[Conflict]) -> Move {
  {walk: {...walk, phase: Closed, pending: []}, action: Finished({settled: settled, owned: owned, conflicts: conflicts})}
}

pure settled_for(conflicts: List[Conflict]) -> Settled {
  if conflicts.is_empty() { Converged } else { Conflicted }
}

pure plan_from(walk: Walk, observed: Observation) -> Move {
  let planned = plan(walk.desired, walk.metric, walk.owned, observed)
  if planned.operations.is_empty() {
    if planned.owned_after == walk.owned {
      return close(walk, settled_for(planned.conflicts), planned.owned_after, planned.conflicts)
    }
    return {walk: {...walk, phase: Saving, plan: planned, intent: planned.owned_after}, action: DoPersist({owned: planned.owned_after, reconcile: false})}
  }
  let intent = union_owned(walk.owned, planned.owned_after)
  {walk: {...walk, phase: Logging, plan: planned, intent: intent}, action: DoPersist({owned: intent, reconcile: true})}
}

pure retry_from(walk: Walk, observed: Observation) -> Move {
  let next = {...walk, owned: walk.intent, round: walk.round + 1, plan: null, pending: []}
  return close(next, Stale, next.intent, []) when next.round >= MAX_ROUNDS
  plan_from(next, observed)
}

pure next_operation(walk: Walk, rest: List[Operation]) -> Move {
  guard ! rest.is_empty() else { return {walk: {...walk, phase: Confirming, pending: []}, action: DoObserve} }
  {walk: {...walk, phase: Applying, pending: rest}, action: DoMutate(rest[0])}
}

pure planned_conflicts(walk: Walk) -> List[Conflict] {
  guard let planned = walk.plan else { return [] }
  planned.conflicts
}

## Starts reconciling the interface toward `desired` (null withdraws all owned
## state) from the owned record `owned`; the first action is an observation.
export pure begin_walk(desired: machine.Lease?, metric: Int, owned: Owned) -> Move {
  let walk = Walk(phase: Observing, desired:, metric:, owned:, round: 0, plan: null, pending: [], intent: owned)
  {walk: walk, action: DoObserve}
}

## Advances a reconciliation by the result of its last action. A feedback that
## does not fit the phase ends the reconciliation as partial rather than
## guessing. After a failed operation the written intent stays in the record
## so the next reconciliation re-plans from observation.
export pure advance(walk: Walk, feedback: Feedback) -> Move {
  match walk.phase {
    Observing => {
      if let Observed(observed) = feedback { plan_from(walk, observed) } else { close(walk, Partial("unexpected feedback while observing"), walk.intent, []) }
    }
    Logging => {
      if let Persisted = feedback { {walk: {...walk, phase: Rechecking}, action: DoObserve} } else { close(walk, Partial("unexpected feedback while logging"), walk.intent, planned_conflicts(walk)) }
    }
    Rechecking => {
      guard let planned = walk.plan else { return close(walk, Partial("missing plan"), walk.intent, []) }
      if let Observed(fresh) = feedback {
        if fresh == planned.before { next_operation(walk, planned.operations) } else { retry_from(walk, fresh) }
      } else {
        close(walk, Partial("unexpected feedback while rechecking"), walk.intent, planned.conflicts)
      }
    }
    Applying => {
      if let MutationFailed(reason) = feedback {
        close(walk, Partial(reason), walk.intent, planned_conflicts(walk))
      } else if let Mutated = feedback {
        next_operation(walk, walk.pending |> drop(1))
      } else {
        close(walk, Partial("unexpected feedback while applying"), walk.intent, planned_conflicts(walk))
      }
    }
    Confirming => {
      guard let planned = walk.plan else { return close(walk, Partial("missing plan"), walk.intent, []) }
      if let Observed(after) = feedback {
        let leftovers = verify(planned, after)
        if leftovers.is_empty() {
          {walk: {...walk, phase: Saving, intent: planned.owned_after}, action: DoPersist({owned: planned.owned_after, reconcile: false})}
        } else if walk.round + 1 >= MAX_ROUNDS {
          close(walk, Partial(f"did not converge: {leftovers.join("; ")}"), walk.intent, planned.conflicts)
        } else {
          retry_from(walk, after)
        }
      } else {
        close(walk, Partial("unexpected feedback while confirming"), walk.intent, planned.conflicts)
      }
    }
    Saving => {
      if let Persisted = feedback {
        close(walk, settled_for(planned_conflicts(walk)), walk.intent, planned_conflicts(walk))
      } else {
        close(walk, Partial("unexpected feedback while saving"), walk.intent, planned_conflicts(walk))
      }
    }
    Closed => close(walk, Partial("reconciliation already closed"), walk.intent, [])
  }
}
