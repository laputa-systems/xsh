##! The DHCPv4 client loop. It owns no protocol decision: the state machine in
##! `dhcp_machine` decides, the wire and frame libraries validate what arrives,
##! the network and DNS libraries change the host. This module wires them to
##! one `ClientIo`, the record of host operations the loop uses, so a test can
##! run the whole loop against a scripted world and production runs it against
##! sockets. The record is passed explicitly; nothing selects it from the
##! environment.
use dhcp_arp as arp
use dhcp_dns as dns_owner
use dhcp_frame as frame_codec
use dhcp_machine as machine
use dhcp_net as network
use dhcp_store as store
use dhcp_wire as wire

## Where the client keeps things and how it behaves. `scratch` is a directory
## only a test sets, for a fake `ClientIo` to keep its world in; the command
## line never fills it.
export type Settings = {
  interface: Str,
  lease_dir: Path,
  run_dir: Path,
  resolv_dir: Path,
  resolv_name: Str,
  static_path: Path?,
  metric: Int,
  timing: machine.Timing,
  scratch: Path?,
}

## The facts of the interface the client was started for.
export type Link = {name: Str, ifindex: Int, mac: Bytes, identity: wire.Identity}

## The descriptors the live `ClientIo` waits on, reads and writes; a fake leaves them -1.
export type Sockets = {udp: Int, packets: Int, arp: Int, events: Int, control: Int}

## Everything a `ClientIo` procedure needs to know about this client.
export type Context = {settings: Settings, link: Link, sockets: Sockets, boot_id: Str}

## What became readable. `control` means the operator asked for something.
export type Ready = {dhcp: Bool, arp: Bool, events: Bool, control: Bool}

## What the operator can ask of a running client.
export enum Request {
  AdminDown,
  AdminUp,
  Stop,
}

## One thing a wait delivered.
export enum Incoming {
  GotFrame(Bytes),
  GotArp(Bytes),
  LinkChanged,
  LinkLost,
  Asked(Request),
}

## One DHCP message to send. `destination` is the server address for unicast.
## `unaddressed` says the client has no address to send from, so the message
## must leave as a raw frame with source 0.0.0.0.
export type Datagram = {bytes: Bytes, destination: Int, broadcast: Bool, unaddressed: Bool}

## One thing the client sends.
export enum Outgoing {
  SendDhcp(Datagram),
  SendArp(Bytes),
}

## A request to bring the owned configuration to a lease (null withdraws it).
export type ReconcileRequest = {record: store.LeaseRecord, desired: machine.Lease?, metric: Int}

## One change the loop asks the host to make.
export enum Change {
  Reconcile(ReconcileRequest),
  Dns(dns_owner.Contribution?),
  Persist(store.LeaseRecord),
  Publish(Str),
}

## What a change did.
export enum Applied {
  Reconciled(network.Outcome),
  Resolver(dns_owner.Standing),
  Done,
}

## The host operations the loop uses. Each takes the `Context` first.
export type ClientIo = {
  now: proc(ctx: Context) [fs, process, net, time, error] -> Result[Int, Error],
  wall: proc(ctx: Context) [fs, process, net, time, error] -> Result[Int, Error],
  random: proc(ctx: Context) [fs, process, net, time, error] -> Result[Int, Error],
  wait: proc(ctx: Context, timeout_ms: Int?) [fs, process, net, time, error] -> Result[Ready, Error],
  receive: proc(ctx: Context) [fs, process, net, time, error] -> Result[List[Incoming], Error],
  send: proc(ctx: Context, outgoing: Outgoing) [fs, process, net, time, error] -> Result[Unit, Error],
  observe: proc(ctx: Context) [fs, process, net, time, error] -> Result[network.Observation, Error],
  apply: proc(ctx: Context, change: Change) [fs, process, net, time, error] -> Result[Applied, Error],
  rebind: proc(ctx: Context, observed: network.Observation) [fs, process, net, time, error] -> Result[Context, Error],
}

## How the loop ends.
export enum Mode {
  Supervised,
  OneShot(Int),
}

## What the loop remembers besides the machine.
export type Session = {
  machine: machine.Machine,
  record: store.LeaseRecord,
  observed: network.Observation,
  counters: Map[Str, Int],
  resolver: Str,
  conflicts: List[network.Conflict],
  stopped: Bool,
  resync: Bool,
  context: Context,
}

## A session after one event, with the machine's effects still to perform.
export type Reaction = {session: Session, effects: List[machine.Effect]}

## Where a run starts from: the remembered record and the first observation.
export type Beginning = {record: store.LeaseRecord?, observed: network.Observation, admin_down: Bool}

## Why a one-shot run ended.
export error ClientFailure {
  NoLease(message: Str)
  Unreachable(message: Str)
}

const MAX_SETTLE_ROUNDS = 8
const MAX_INCOMING = 64
const RESYNC_MS = 500
const BROADCAST = 4294967295

pure unaddressed(kind: machine.PacketKind) -> Bool {
  match kind {
    machine.Discover => true
    machine.RequestSelecting => true
    machine.RequestInitReboot => true
    machine.Decline => true
    machine.RequestBound => false
    machine.Release => false
  }
}

pure bump(counters: Map[Str, Int], key: Str) -> Map[Str, Int] {
  counters.set(key, (counters.get(key) ?? 0) + 1)
}

## The text of a message the machine asked to send.
export pure encode_packet(identity: wire.Identity, packet: machine.Packet) -> Result[Bytes, Error] {
  match packet.kind {
    machine.Discover => wire.discover(identity, packet.xid, packet.secs)
    machine.RequestSelecting => wire.request_selecting(identity, packet.xid, packet.secs, packet.address, packet.server)
    machine.RequestInitReboot => wire.request_init_reboot(identity, packet.xid, packet.secs, packet.address)
    machine.RequestBound => wire.request_bound(identity, packet.xid, packet.secs, packet.address)
    machine.Decline => wire.decline(identity, packet.xid, packet.address, packet.server)
    machine.Release => wire.release(identity, packet.xid, packet.address, packet.server)
  }
}

## Runs one event through the machine at `now` and counts what it ignored.
export pure feed(session: Session, event: machine.Event, now: Int) -> Reaction {
  let step = machine.transition(session.machine, event, now)
  var counters = session.counters
  for key in step.note.split(";") {
    counters = bump(counters, key) when key != ""
  }
  {session: {...session, machine: step.machine, counters: counters}, effects: step.effects}
}

## The machine event, or a counted rejection, for one frame received while the
## machine waits for the transaction it has open.
export pure frame_event(session: Session, link: Link, frame: Bytes, now: Int) -> Reaction {
  match frame_codec.server_datagram(frame, link.mac) {
    Ok(found) => {
      match wire.accept_reply(found.payload, link.identity, session.machine.xid) {
        Ok(message) => feed(session, .OnReceived(message), now)
        Err(reject) => {reaction_of(session, bump(session.counters, f"reject_{wire.reject_reason(reject)}"))}
      }
    }
    Err(reject) => {reaction_of(session, bump(session.counters, frame_codec.reject_reason(reject)))}
  }
}

pure reaction_of(session: Session, counters: Map[Str, Int]) -> Reaction {
  {session: {...session, counters: counters}, effects: []}
}

## The machine event for an ARP frame: a conflict while a candidate address is
## being probed or after it is configured.
export pure arp_event(session: Session, link: Link, frame: Bytes, now: Int) -> Reaction {
  let candidate = session.machine.candidate
  let held = if candidate == null { session.machine.lease } else { candidate }
  guard let lease = held else { return reaction_of(session, session.counters) }
  if arp.conflicts(frame, link.mac, lease.address) {
    return feed(session, .OnArpConflict, now)
  }
  reaction_of(session, session.counters)
}

## The events a fresh observation implies against the previous one: a lost
## carrier, a returned carrier, or a recreated interface (a changed index).
export pure link_events(state: machine.State, before: network.Observation, after: network.Observation) -> List[machine.Event] {
  return [.OnLinkDown, .OnLinkUp(true)] when after.ifindex != before.ifindex and after.carrier
  return [.OnLinkDown] when after.ifindex != before.ifindex
  return [.OnLinkDown] when ! after.carrier
  return [.OnLinkUp(false)] when state == .NoLink
  []
}

## True when the machine holds a lease whose owned address is missing from the
## observation, which means something outside removed it and the client should
## assert its configuration again.
export pure needs_repair(session: Session, fresh: network.Observation) -> Bool {
  guard let lease = session.machine.lease else { return false }
  return false unless session.machine.state == .Bound or session.machine.state == .Renewing or session.machine.state == .Rebinding
  for item in session.record.owned.addresses {
    return false when item.address == lease.address and network.holds_address(fresh.addresses, item)
  }
  true
}

pure status_text(session: Session, report: machine.Report, wall: Int) -> Str {
  json.encode(
    {
      state: report.state,
      address: if let address = report.address { wire.ip_text(address) } else { null },
      prefix: report.prefix,
      server: if let address = report.server { wire.ip_text(address) } else { null },
      renew_at: report.renew_at,
      rebind_at: report.rebind_at,
      expire_at: report.expire_at,
      resolver: session.resolver,
      conflicts: [f"{conflict.kind} {conflict.subject}" for conflict in session.conflicts],
      counters: session.counters,
      wall_ms: wall,
    },
  ) ?? "{}"
}

proc settle_one(host: ClientIo, ctx: Context, session: Session, effect: machine.Effect, now: Int) [fs, process, net, time, error] -> Result[Reaction, Error] {
  match effect {
    machine.Send(packet) => {
      let datagram = encode_packet(ctx.link.identity, packet)?
      host.send(ctx, .SendDhcp({bytes: datagram, destination: if packet.unicast { packet.server } else { BROADCAST }, broadcast: ! packet.unicast, unaddressed: unaddressed(packet.kind)}))
      Ok(reaction_of(session, bump(session.counters, f"sent_{machine.kind_name(packet.kind)}")))
    }
    machine.Probe(address) => {
      host.send(ctx, .SendArp(arp.probe(ctx.link.mac, address)?))
      Ok(reaction_of(session, bump(session.counters, "sent_arp_probe")))
    }
    machine.Announce(address) => {
      host.send(ctx, .SendArp(arp.announce(ctx.link.mac, address)?))
      Ok(reaction_of(session, bump(session.counters, "sent_arp_announce")))
    }
    machine.Configure(lease) => configure(host, ctx, session, lease, now)
    machine.Deconfigure(_) => withdraw(host, ctx, session, now)
    machine.Persist(lease) => {
      let record = {...session.record, lease: lease, boot_id: ctx.boot_id, boottime_ms: now}
      let _ = host.apply(ctx, .Persist(record))?
      Ok({session: {...session, record: record}, effects: []})
    }
    machine.Forget => {
      let record = {...session.record, lease: null, boot_id: ctx.boot_id, boottime_ms: now}
      let _ = host.apply(ctx, .Persist(record))?
      Ok({session: {...session, record: record}, effects: []})
    }
    machine.Publish(report) => {
      let wall = host.wall(ctx)?
      let _ = host.apply(ctx, .Publish(status_text(session, report, wall)))?
      Ok(reaction_of(session, session.counters))
    }
    machine.Arm(_) => Ok(reaction_of(session, session.counters))
    machine.Disarm => Ok(reaction_of(session, session.counters))
  }
}

proc configure(host: ClientIo, ctx: Context, session: Session, lease: machine.Lease, now: Int) [fs, process, net, time, error] -> Result[Reaction, Error] {
  let base = {...session.record, lease: lease, boot_id: ctx.boot_id, boottime_ms: now}
  let changed = host.apply(ctx, .Reconcile({record: base, desired: lease, metric: ctx.settings.metric}))?
  let outcome = outcome_of(changed)?
  let owned_record = {...base, owned: outcome.owned, reconcile: false}
  let converged = outcome.settled == .Converged
  var counters = session.counters
  var resolver = session.resolver
  if converged {
    let standing = host.apply(ctx, .Dns(dns_owner.contribution_for(ctx.settings.interface, lease)))?
    if let .Resolver(published) = standing {
      resolver = standing_name(published)
    }
  } else {
    counters = bump(counters, "configure_failed")
  }
  let adjusted = {...session, record: owned_record, counters: counters, resolver: resolver, conflicts: outcome.conflicts}
  let followed = feed(adjusted, .OnApplied(converged), now)
  Ok(followed)
}

proc withdraw(host: ClientIo, ctx: Context, session: Session, now: Int) [fs, process, net, time, error] -> Result[Reaction, Error] {
  let base = {...session.record, lease: null, boot_id: ctx.boot_id, boottime_ms: now}
  let changed = host.apply(ctx, .Reconcile({record: base, desired: null, metric: ctx.settings.metric}))?
  let outcome = outcome_of(changed)?
  let standing = host.apply(ctx, .Dns(null))?
  let resolver = if let .Resolver(published) = standing { standing_name(published) } else { session.resolver }
  Ok({session: {...session, record: {...base, owned: outcome.owned, reconcile: false}, resolver: resolver, conflicts: outcome.conflicts}, effects: []})
}

pure outcome_of(applied: Applied) -> Result[network.Outcome, Error] {
  match applied {
    Reconciled(outcome) => Ok(outcome)
    Resolver(_) => Err(error.failure("a reconcile request must answer with an outcome"))
    Done => Err(error.failure("a reconcile request must answer with an outcome"))
  }
}

pure standing_name(standing: dns_owner.Standing) -> Str {
  match standing {
    dns_owner.Published => "published"
    dns_owner.Unchanged => "unchanged"
    dns_owner.Conflict(reason) => f"dns-conflict {reason}"
  }
}

## Performs the effects of a reaction, feeding the events they cause back
## through the machine until nothing is left to do or the bound is reached.
export proc settle(host: ClientIo, ctx: Context, reaction: Reaction, now: Int) [fs, process, net, time, error] -> Result[Session, Error] {
  var session = reaction.session
  var queue = reaction.effects
  var rounds = 0
  while ! queue.is_empty() and rounds < MAX_SETTLE_ROUNDS {
    let current = queue
    queue = []
    for effect in current {
      let done = settle_one(host, ctx, session, effect, now)?
      session = done.session
      queue += done.effects
    }
    rounds += 1
  }
  Ok(session)
}

proc handle(host: ClientIo, ctx: Context, session: Session, incoming: Incoming, now: Int) [fs, process, net, time, error] -> Result[Session, Error] {
  match incoming {
    GotFrame(frame) => settle(host, ctx, frame_event(session, ctx.link, frame, now), now)
    GotArp(frame) => settle(host, ctx, arp_event(session, ctx.link, frame, now), now)
    LinkChanged => observed_change(host, ctx, session, now)
    LinkLost => observed_change(host, ctx, bump_session(session, "events_lost"), now)
    Asked(command) => settle(host, ctx, feed(session, command_event(command), now), now)
  }
}

pure bump_session(session: Session, key: Str) -> Session {
  {...session, counters: bump(session.counters, key)}
}

pure command_event(command: Request) -> machine.Event {
  match command {
    AdminDown => .OnAdminDown
    AdminUp => .OnAdminUp
    Stop => .OnShutdown(false)
  }
}

proc observed_change(host: ClientIo, ctx: Context, session: Session, now: Int) [fs, process, net, time, error] -> Result[Session, Error] {
  let observation = host.observe(ctx)
  if let Err(_) = observation {
    let missing = {...bump_session(session, "observe_failed"), resync: true}
    return settle(host, ctx, feed(missing, .OnLinkDown, now), now)
  }
  let fresh = observation?
  let recreated = fresh.ifindex != session.observed.ifindex
  let bound = if recreated { host.rebind(ctx, fresh)? } else { ctx }
  let record = if recreated { {...session.record, ifindex: fresh.ifindex, owned: network.empty_owned(fresh.ifindex)} } else { session.record }
  var current = {...session, observed: fresh, context: bound, record: record, resync: false}
  for event in link_events(session.machine.state, session.observed, fresh) {
    current = settle(host, bound, feed(current, event, now), now)?
  }
  if needs_repair(current, fresh) {
    if let lease = current.machine.lease {
      let repaired = configure(host, bound, bump_session(current, "repaired"), lease, now)?
      current = settle(host, bound, repaired, now)?
    }
  }
  Ok(current)
}

pure begin(ctx: Context, beginning: Beginning) -> Session {
  let {settings, link, boot_id, ..} = ctx
  let revived = store.revalidate(
    beginning.record,
    store.Current(name: link.name, ifindex: link.ifindex, mac: link.mac, boot_id:),
  )
  let record = store.LeaseRecord(
    name: link.name,
    ifindex: link.ifindex,
    mac: link.mac,
    client_id: link.identity.client_id,
    boot_id:,
    boottime_ms: 0,
    lease: revived.lease,
    owned: revived.owned,
    reconcile: revived.reconcile,
  )
  {
    machine: machine.new_machine(settings.timing),
    record: record,
    observed: beginning.observed,
    counters: map.empty(),
    resolver: "none",
    conflicts: [],
    stopped: false,
    resync: false,
    context: ctx,
  }
}

## Runs the client: starts the machine from the remembered record, then waits,
## receives, feeds and settles until stopped. A supervised run returns only on
## a stop command or an error; a one-shot run returns once a lease is bound or
## its deadline passes.
export proc drive(host: ClientIo, ctx: Context, beginning: Beginning, mode: Mode) [fs, process, net, time, error] -> Result[Session, Error] {
  var session = begin(ctx, beginning)
  let revived = store.revalidate(beginning.record, store.Current(name: ctx.link.name, ifindex: ctx.link.ifindex, mac: ctx.link.mac, boot_id: ctx.boot_id))
  var now = host.now(ctx)?
  let info = machine.StartInfo(
    seed: host.random(ctx)?,
    carrier: beginning.observed.carrier,
    remembered: revived.lease,
    configured: store.configured(revived, beginning.observed),
  )
  session = settle(host, ctx, feed(session, .OnStart(info), now), now)?
  if beginning.admin_down {
    session = settle(host, ctx, feed(session, .OnAdminDown, now), now)?
  }
  let deadline = if let .OneShot(limit) = mode { now + limit } else { null }
  while ! session.stopped {
    let live = session.context
    session = observed_change(host, live, session, now)? when session.resync
    if let limit = deadline {
      return Ok(session) when session.machine.state == .Bound
      return Err(ClientFailure.NoLease("no lease before the deadline")) when now >= limit
    }
    let wake = if let at = session.machine.wake_at { at - now } else { null }
    let limited = if let limit = deadline { min_wait(wake, limit - now) } else { wake }
    let bounded = if session.resync { min_wait(limited, RESYNC_MS) } else { limited }
    let ready = host.wait(live, bounded)?
    now = host.now(live)?
    if ready.dhcp or ready.arp or ready.events or ready.control {
      let delivered = host.receive(live)?
      for incoming in delivered |> take(MAX_INCOMING) {
        session = handle(host, live, session, incoming, now)?
        if let .Asked(command) = incoming {
          if command == Stop {
            session = {...session, stopped: true}
          }
        }
      }
    }
    if let at = session.machine.wake_at {
      if now >= at {
        session = settle(host, live, feed(session, .OnTick, now), now)?
      }
    }
  }
  Ok(session)
}

pure min_wait(left: Int?, right: Int) -> Int {
  guard let value = left else { return right }
  if value < right { value } else { right }
}
