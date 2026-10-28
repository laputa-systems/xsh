##! The DHCPv4 client as a pure state machine (RFC 2131 section 4.4). One
##! function, `transition`, maps a machine, one validated event and the
##! current time to the next machine and a short, closed list of effects. The
##! module holds no clock, socket or file: time arrives as an argument, as
##! integer milliseconds of a clock that keeps counting through suspend, and
##! randomness is a seeded generator inside the machine, so every run replays.
use dhcp_wire as wire

## The most effects one transition may return.
export const MAX_EFFECTS = 8

const FAR_FUTURE = 4611686018427387904
const MAX_ATTEMPT_SHIFT = 20
const MAX_SECS = 65535

## Where the client is in the protocol. `Probing` is the address-conflict
## check between an acknowledgement and configuration; `NoLink` is a lost
## carrier; `AdminDown` is an operator release.
export enum State: Str {
  Init = "init",
  Selecting = "selecting",
  Requesting = "requesting",
  Probing = "probing",
  Bound = "bound",
  Renewing = "renewing",
  Rebinding = "rebinding",
  InitReboot = "init_reboot",
  Declined = "declined",
  AdminDown = "admin_down",
  NoLink = "no_link",
}

## The DHCP message a send effect asks the caller to encode.
export enum PacketKind: Str {
  Discover = "discover",
  RequestSelecting = "request_selecting",
  RequestInitReboot = "request_init_reboot",
  RequestBound = "request_bound",
  Decline = "decline",
  Release = "release",
}

## One message to send. `unicast` names the server to send to; otherwise the
## message is broadcast. `address` and `server` are zero where the kind has
## none.
export type Packet = {kind: PacketKind, xid: Int, secs: Int, address: Int, server: Int, unicast: Bool}

## Why configuration is withdrawn.
export enum Reason: Str {
  NakReceived = "nak",
  LeaseExpired = "expired",
  AdminRequest = "admin_down",
  ApplyFailure = "apply_failed",
  LinkRecreated = "link_recreated",
}

## How the renewal and rebinding times of a lease were chosen.
export enum TimerRule: Str {
  Infinite = "infinite",
  Server = "server",
  Mixed = "mixed",
  Default = "default",
  Repaired = "repaired",
}

## Renewal and rebinding delays from the start of a lease, in milliseconds.
export type Timers = {renew_ms: Int, rebind_ms: Int, rule: TimerRule}

## Why an acknowledged lease cannot be used.
export error LeaseReject {
  ZeroLease
  ShortLease(seconds: Int)
  BadAddress
}

## Delays and counts, in milliseconds. Tests shrink them; the client uses
## `default_timing`.
export type Timing = {
  backoff_initial_ms: Int,
  backoff_max_ms: Int,
  jitter_ms: Int,
  min_retry_ms: Int,
  decline_delay_ms: Int,
  nak_delay_ms: Int,
  request_attempts: Int,
  reboot_attempts: Int,
  probe_count: Int,
  probe_min_ms: Int,
  probe_max_ms: Int,
  announce_wait_ms: Int,
  announce_interval_ms: Int,
  announce_count: Int,
  min_lease_secs: Int,
}

## The delays of RFC 2131 and RFC 5227: a four-second first retransmission
## doubling to 64 seconds with one second of jitter, three probes and two
## announcements.
export pure default_timing() -> Timing {
  Timing(
    backoff_initial_ms: 4000,
    backoff_max_ms: 64000,
    jitter_ms: 1000,
    min_retry_ms: 60000,
    decline_delay_ms: 10000,
    nak_delay_ms: 4000,
    request_attempts: 4,
    reboot_attempts: 3,
    probe_count: 3,
    probe_min_ms: 1000,
    probe_max_ms: 2000,
    announce_wait_ms: 2000,
    announce_interval_ms: 2000,
    announce_count: 2,
    min_lease_secs: 2,
  )
}

## A lease the interface uses or is being given. Times are absolute readings
## of the clock the machine runs on; null means the lease never ends.
export type Lease = {
  address: Int,
  prefix: Int,
  broadcast: Int,
  routers: List[Int],
  dns: List[Int],
  domain: Str?,
  server: Int,
  lease_secs: Int,
  acquired_at: Int,
  renew_at: Int?,
  rebind_at: Int?,
  expire_at: Int?,
  timers: TimerRule,
}

## The server and address a client asks for again after a reboot or a lost
## link.
export type Offer = {address: Int, server: Int}

## What the machine publishes for status and for supervisors.
export type Report = {
  state: Str,
  address: Int?,
  prefix: Int?,
  server: Int?,
  renew_at: Int?,
  rebind_at: Int?,
  expire_at: Int?,
}

## The circumstances of a process start. `remembered` is the lease a previous
## run left; `configured` says the interface still carries it; `seed` starts
## the machine's generator.
export type StartInfo = {seed: Int, carrier: Bool, remembered: Lease?, configured: Bool}

## Everything the machine reacts to. Messages arrive already validated and
## matched to this client by the wire library.
export enum Event {
  OnStart(StartInfo),
  OnReceived(wire.Message),
  OnTick,
  OnLinkUp(Bool),
  OnLinkDown,
  OnArpConflict,
  OnApplied(Bool),
  OnAdminUp,
  OnAdminDown,
  OnShutdown(Bool),
}

## Everything the machine asks the caller to do. `Arm` names the next time the
## machine wants a tick; `Disarm` cancels it.
export enum Effect {
  Send(Packet),
  Probe(Int),
  Announce(Int),
  Configure(Lease),
  Deconfigure(Reason),
  Persist(Lease),
  Forget,
  Publish(Report),
  Arm(Int),
  Disarm,
}

## The machine's whole memory.
export type Machine = {
  state: State,
  timing: Timing,
  rng: Int,
  xid: Int,
  attempt: Int,
  began_at: Int,
  request_sent_at: Int,
  offer: Offer?,
  lease: Lease?,
  candidate: Lease?,
  carrier: Bool,
  probes_sent: Int,
  announces_left: Int,
  retry_at: Int?,
  wake_at: Int?,
  last_now: Int,
}

## One transition: the next machine, the effects in order, and a stable name
## for an event the machine ignored or a repair it made ("" for none).
export type Step = {machine: Machine, effects: List[Effect], note: Str}


type Draw = {machine: Machine, value: Int}

type Dropped = {machine: Machine, effects: List[Effect]}

## A machine at rest before `OnStart`.
export pure new_machine(timing: Timing) -> Machine {
  Machine(
    state: Init,
    timing:,
    rng: 1,
    xid: 0,
    attempt: 0,
    began_at: 0,
    request_sent_at: 0,
    offer: null,
    lease: null,
    candidate: null,
    carrier: true,
    probes_sent: 0,
    announces_left: 0,
    retry_at: null,
    wake_at: null,
    last_now: 0,
  )
}

## The sum of two non-negative times, capped so it can never overflow.
export pure add_ms(base: Int, delta: Int) -> Int {
  return FAR_FUTURE when delta >= FAR_FUTURE - base
  base + delta
}

pure classful_mask(address: Int) -> Int {
  let first = address / 16777216
  return 4278190080 when first < 128
  return 4294901760 when first < 192
  4294967040
}

pure on_subnet(address: Int, other: Int, mask: Int) -> Bool {
  let host_bits = 4294967295 - mask
  address - address.bit_and(host_bits) == other - other.bit_and(host_bits)
}

## The renewal and rebinding delays of a finite lease in milliseconds, and the
## rule that chose them. A server value is used when it is positive and below
## the lease; renewal must precede rebinding, and a pair that does not is
## replaced by the defaults of one half and seven eighths of the lease.
export pure lease_timers(lease_secs: Int, server_renewal: Int?, server_rebinding: Int?) -> Timers {
  let lease_ms = lease_secs * 1000
  let default_renew = lease_ms / 2
  let default_rebind = lease_ms / 8 * 7
  let renewal = server_renewal ?? 0
  let rebinding = server_rebinding ?? 0
  let usable_renewal = renewal > 0 and renewal < lease_secs
  let usable_rebinding = rebinding > 0 and rebinding < lease_secs
  let renew_ms = if usable_renewal { renewal * 1000 } else { default_renew }
  let rebind_ms = if usable_rebinding { rebinding * 1000 } else { default_rebind }
  let offered = server_renewal != null or server_rebinding != null
  if renew_ms >= rebind_ms {
    return {renew_ms: default_renew, rebind_ms: default_rebind, rule: if offered { Repaired } else { Default }}
  }
  let rule = if usable_renewal and usable_rebinding {
    Server
  } else if usable_renewal or usable_rebinding {
    Mixed
  } else if offered {
    Repaired
  } else {
    Default
  }
  {renew_ms: renew_ms, rebind_ms: rebind_ms, rule: rule}
}

## Builds the lease an acknowledgement grants. `anchor` is when the request
## it answers was first sent, which is when the lease starts. A lease of zero
## or under `min_lease_secs` seconds, or an address the mask makes a network or
## broadcast address, is rejected. A missing mask falls back to the classful
## one, and only routers on the leased subnet are kept.
export pure lease_from_ack(message: wire.Message, anchor: Int, timing: Timing) -> Result[Lease, LeaseReject] {
  let lease_secs = message.lease_secs ?? 0
  fail .ZeroLease() when lease_secs == 0
  let infinite = lease_secs == wire.INFINITE_LEASE
  fail .ShortLease(lease_secs) when ! infinite and lease_secs < timing.min_lease_secs
  let mask = message.subnet_mask ?? classful_mask(message.yiaddr)
  let prefix = wire.mask_prefix(mask) ?? 0
  let host_bits = 4294967295 - mask
  let host = message.yiaddr.bit_and(host_bits)
  fail .BadAddress() when prefix < 31 and (host == 0 or host == host_bits)
  fail .BadAddress() when prefix == 0
  let routers = [router for router in message.routers if router != message.yiaddr and on_subnet(router, message.yiaddr, mask)]
  let broadcast = message.yiaddr - host + host_bits
  if infinite {
    return Ok(
      Lease(
        address: message.yiaddr,
        prefix:,
        broadcast:,
        routers:,
        dns: message.dns,
        domain: message.domain,
        server: message.server_id ?? 0,
        lease_secs:,
        acquired_at: anchor,
        renew_at: null,
        rebind_at: null,
        expire_at: null,
        timers: Infinite,
      ),
    )
  }
  let timers = lease_timers(lease_secs, message.renewal_secs, message.rebinding_secs)
  Ok(
    Lease(
      address: message.yiaddr,
      prefix:,
      broadcast:,
      routers:,
      dns: message.dns,
      domain: message.domain,
      server: message.server_id ?? 0,
      lease_secs:,
      acquired_at: anchor,
      renew_at: add_ms(anchor, timers.renew_ms),
      rebind_at: add_ms(anchor, timers.rebind_ms),
      expire_at: add_ms(anchor, lease_secs * 1000),
      timers: timers.rule,
    ),
  )
}

## The status a machine publishes.
export pure status(machine: Machine) -> Report {
  let lease = machine.lease
  Report(
    state: state_name(machine.state),
    address: lease?.address,
    prefix: lease?.prefix,
    server: lease?.server,
    renew_at: lease?.renew_at,
    rebind_at: lease?.rebind_at,
    expire_at: lease?.expire_at,
  )
}

## The stable name of a state.
export pure state_name(state: State) -> Str {
  match state {
    Init => "init"
    Selecting => "selecting"
    Requesting => "requesting"
    Probing => "probing"
    Bound => "bound"
    Renewing => "renewing"
    Rebinding => "rebinding"
    InitReboot => "init_reboot"
    Declined => "declined"
    AdminDown => "admin_down"
    NoLink => "no_link"
  }
}

## The stable name of a packet kind.
export pure kind_name(kind: PacketKind) -> Str {
  match kind {
    Discover => "discover"
    RequestSelecting => "request_selecting"
    RequestInitReboot => "request_init_reboot"
    RequestBound => "request_bound"
    Decline => "decline"
    Release => "release"
  }
}

## The stable name of a withdrawal reason.
export pure reason_name(reason: Reason) -> Str {
  match reason {
    NakReceived => "nak"
    LeaseExpired => "expired"
    AdminRequest => "admin_down"
    ApplyFailure => "apply_failed"
    LinkRecreated => "link_recreated"
  }
}

pure next_random(state: Int) -> Int {
  (state * 1103515245 + 12345) % 2147483648
}

pure draw(machine: Machine, limit: Int) -> Draw {
  let rng = next_random(machine.rng)
  {machine: {...machine, rng}, value: rng / 8 % limit}
}

pure draw_xid(machine: Machine) -> Draw {
  let high = draw(machine, 65536)
  let low = draw(high.machine, 65536)
  let xid = high.value * 65536 + low.value
  {machine: low.machine, value: if xid == 0 { 1 } else { xid }}
}

pure backoff(machine: Machine, attempt: Int) -> Draw {
  let timing = machine.timing
  var base = timing.backoff_initial_ms
  var step = 0
  while step < attempt and step < MAX_ATTEMPT_SHIFT and base < timing.backoff_max_ms {
    base *= 2
    step += 1
  }
  let capped = if base > timing.backoff_max_ms { timing.backoff_max_ms } else { base }
  let jittered = draw(machine, 2 * timing.jitter_ms + 1)
  let delay = capped + jittered.value - timing.jitter_ms
  {machine: jittered.machine, value: if delay < 1 { 1 } else { delay }}
}

pure uniform(machine: Machine, low: Int, high: Int) -> Draw {
  return {machine: machine, value: low} when high <= low
  let drawn = draw(machine, high - low + 1)
  {machine: drawn.machine, value: low + drawn.value}
}

pure seconds_since(started: Int, now: Int) -> Int {
  return 0 when now <= started
  let seconds = (now - started) / 1000
  if seconds > MAX_SECS { MAX_SECS } else { seconds }
}

pure packet(machine: Machine, kind: PacketKind, now: Int, address: Int, server: Int, unicast: Bool) -> Packet {
  {kind: kind, xid: machine.xid, secs: seconds_since(machine.began_at, now), address: address, server: server, unicast: unicast}
}

pure step_of(machine: Machine, effects: List[Effect], note: Str) -> Step {
  {machine: machine, effects: effects, note: note}
}

pure with_retry(delay: Draw, now: Int) -> Machine {
  {...delay.machine, retry_at: add_ms(now, delay.value)}
}

pure offer_of(lease: Lease) -> Offer {
  {address: lease.address, server: lease.server}
}

pure begin_discovery(machine: Machine, now: Int) -> Step {
  let identified = draw_xid(machine)
  let started = {...identified.machine, state: Selecting, xid: identified.value, attempt: 0, began_at: now, request_sent_at: now, offer: null, candidate: null, probes_sent: 0, announces_left: 0}
  let waiting = with_retry(backoff(started, 0), now)
  step_of(waiting, [Send(packet(waiting, Discover, now, 0, 0, false))], "")
}

pure begin_wait(machine: Machine, delay_ms: Int, now: Int, note: Str) -> Step {
  let waiting = {...machine, state: Init, offer: null, candidate: null, probes_sent: 0, announces_left: 0, retry_at: add_ms(now, delay_ms)}
  step_of(waiting, [], note)
}

pure begin_reboot(machine: Machine, offer: Offer, now: Int) -> Step {
  let identified = draw_xid(machine)
  let started = {...identified.machine, state: InitReboot, xid: identified.value, attempt: 0, began_at: now, request_sent_at: now, offer: offer, candidate: null, probes_sent: 0, announces_left: 0}
  let waiting = with_retry(backoff(started, 0), now)
  step_of(waiting, [Send(packet(waiting, RequestInitReboot, now, offer.address, 0, false))], "")
}

pure begin_probing(machine: Machine, lease: Lease, now: Int) -> Step {
  let interval = uniform(machine, machine.timing.probe_min_ms, machine.timing.probe_max_ms)
  let probing = {...interval.machine, state: Probing, candidate: lease, probes_sent: 1, retry_at: add_ms(now, interval.value)}
  step_of(probing, [Probe(lease.address)], "")
}

pure commit(machine: Machine, lease: Lease, now: Int) -> Step {
  let announcing = machine.timing.probe_count > 0
  let announces = if announcing { machine.timing.announce_count - 1 } else { 0 }
  let announce_at = if announces > 0 { add_ms(now, machine.timing.announce_interval_ms) } else { null }
  let bound = {...machine, state: Bound, lease: lease, candidate: null, offer: null, attempt: 0, probes_sent: 0, announces_left: announces, retry_at: announce_at}
  let announce: List[Effect] = if announcing { [Announce(lease.address)] } else { [] }
  step_of(bound, [Configure(lease), Persist(lease)].extend(announce), "")
}

pure accept_lease(machine: Machine, lease: Lease, now: Int) -> Step {
  if let current = machine.lease {
    return renew_in_place(machine, current, lease) when current.address == lease.address
  }
  return begin_probing(machine, lease, now) when machine.timing.probe_count > 0
  commit(machine, lease, now)
}

pure renew_in_place(machine: Machine, current: Lease, lease: Lease) -> Step {
  let changed = current.prefix != lease.prefix or current.routers != lease.routers or current.dns != lease.dns or current.domain != lease.domain or current.broadcast != lease.broadcast
  let bound = {...machine, state: Bound, lease: lease, candidate: null, offer: null, attempt: 0, probes_sent: 0, announces_left: 0, retry_at: null}
  let configure: List[Effect] = if changed { [Configure(lease)] } else { [] }
  step_of(bound, configure.push(Persist(lease)), if changed { "options_changed" } else { "" })
}

pure drop_lease(machine: Machine, reason: Reason) -> Dropped {
  guard machine.lease != null else { return {machine: machine, effects: []} }
  {machine: {...machine, lease: null}, effects: [Deconfigure(reason), Forget]}
}

pure restart_after(machine: Machine, reason: Reason, delay_ms: Int, now: Int, note: Str) -> Step {
  let dropped = drop_lease(machine, reason)
  let waiting = begin_wait(dropped.machine, delay_ms, now, note)
  step_of(waiting.machine, dropped.effects, note)
}

pure restart_now(machine: Machine, reason: Reason, now: Int, note: Str) -> Step {
  let dropped = drop_lease(machine, reason)
  let discovery = begin_discovery(dropped.machine, now)
  step_of(discovery.machine, dropped.effects.extend(discovery.effects), note)
}

pure lease_due(machine: Machine, now: Int) -> Bool {
  guard let lease = machine.lease else { return false }
  guard let expire_at = lease.expire_at else { return false }
  now >= expire_at
}

pure expire(machine: Machine, now: Int) -> Step {
  restart_now(machine, LeaseExpired, now, "lease_expired")
}

pure send_bound_request(machine: Machine, lease: Lease, state: State, now: Int) -> Step {
  let identified = draw_xid(machine)
  let target = if state == Rebinding { lease.expire_at } else { lease.rebind_at }
  let remaining = (target ?? FAR_FUTURE) - now
  let pause = if remaining <= 0 { machine.timing.min_retry_ms } else { max_of(remaining / 2, machine.timing.min_retry_ms) }
  let next = {...identified.machine, state: state, xid: identified.value, attempt: 0, began_at: now, request_sent_at: now, retry_at: add_ms(now, pause)}
  step_of(next, [Send(packet(next, RequestBound, now, lease.address, lease.server, state == Renewing))], "")
}

pure max_of(left: Int, right: Int) -> Int {
  if left > right { left } else { right }
}

pure resend_bound_request(machine: Machine, lease: Lease, now: Int) -> Step {
  let target = if machine.state == Rebinding { lease.expire_at } else { lease.rebind_at }
  let remaining = (target ?? FAR_FUTURE) - now
  let pause = if remaining <= 0 { machine.timing.min_retry_ms } else { max_of(remaining / 2, machine.timing.min_retry_ms) }
  let next = {...machine, attempt: machine.attempt + 1, retry_at: add_ms(now, pause)}
  step_of(next, [Send(packet(next, RequestBound, now, lease.address, lease.server, machine.state == Renewing))], "")
}

pure on_tick_discovering(machine: Machine, now: Int) -> Step {
  let attempt = machine.attempt + 1
  let delay = backoff(machine, attempt)
  let next = {...with_retry(delay, now), attempt: attempt}
  step_of(next, [Send(packet(next, Discover, now, 0, 0, false))], "")
}

pure on_tick_requesting(machine: Machine, offer: Offer, now: Int) -> Step {
  if machine.attempt + 1 >= machine.timing.request_attempts {
    return begin_discovery(machine, now)
  }
  let attempt = machine.attempt + 1
  let delay = backoff(machine, attempt)
  let next = {...with_retry(delay, now), attempt: attempt}
  step_of(next, [Send(packet(next, RequestSelecting, now, offer.address, offer.server, false))], "")
}

pure on_tick_reboot(machine: Machine, offer: Offer, now: Int) -> Step {
  if machine.attempt + 1 >= machine.timing.reboot_attempts {
    return begin_discovery(machine, now)
  }
  let attempt = machine.attempt + 1
  let delay = backoff(machine, attempt)
  let next = {...with_retry(delay, now), attempt: attempt}
  step_of(next, [Send(packet(next, RequestInitReboot, now, offer.address, 0, false))], "")
}

pure on_tick_probing(machine: Machine, candidate: Lease, now: Int) -> Step {
  let timing = machine.timing
  if machine.probes_sent < timing.probe_count {
    let interval = uniform(machine, timing.probe_min_ms, timing.probe_max_ms)
    let next = {...interval.machine, probes_sent: machine.probes_sent + 1, retry_at: add_ms(now, interval.value)}
    return step_of(next, [Probe(candidate.address)], "")
  }
  if machine.probes_sent == timing.probe_count {
    let next = {...machine, probes_sent: machine.probes_sent + 1, retry_at: add_ms(now, timing.announce_wait_ms)}
    return step_of(next, [], "")
  }
  commit(machine, candidate, now)
}

pure reached(deadline: Int?, now: Int) -> Bool {
  guard let at = deadline else { return false }
  now >= at
}

pure on_tick_bound(machine: Machine, lease: Lease, now: Int) -> Step {
  return send_bound_request(machine, lease, Rebinding, now) when reached(lease.rebind_at, now)
  return send_bound_request(machine, lease, Renewing, now) when reached(lease.renew_at, now)
  if reached(machine.retry_at, now) and machine.announces_left > 0 {
    let more = machine.announces_left > 1
    let announce_at = if more { add_ms(now, machine.timing.announce_interval_ms) } else { null }
    let announcing = {...machine, announces_left: machine.announces_left - 1, retry_at: announce_at}
    return step_of(announcing, [Announce(lease.address)], "")
  }
  step_of(machine, [], "")
}

pure on_tick_renewing(machine: Machine, lease: Lease, now: Int) -> Step {
  return send_bound_request(machine, lease, Rebinding, now) when reached(lease.rebind_at, now)
  return resend_bound_request(machine, lease, now) when reached(machine.retry_at, now)
  step_of(machine, [], "")
}

pure on_tick_rebinding(machine: Machine, lease: Lease, now: Int) -> Step {
  return resend_bound_request(machine, lease, now) when reached(machine.retry_at, now)
  step_of(machine, [], "")
}

pure retry_due(machine: Machine, now: Int) -> Bool {
  reached(machine.retry_at, now)
}

pure on_tick(machine: Machine, now: Int) -> Step {
  let held = machine.lease
  match machine.state {
    Init => if retry_due(machine, now) { begin_discovery(machine, now) } else { step_of(machine, [], "") }
    Declined => if retry_due(machine, now) { begin_discovery(machine, now) } else { step_of(machine, [], "") }
    Selecting => if retry_due(machine, now) { on_tick_discovering(machine, now) } else { step_of(machine, [], "") }
    Requesting => {
      guard let offer = machine.offer else { return begin_discovery(machine, now) }
      if retry_due(machine, now) { on_tick_requesting(machine, offer, now) } else { step_of(machine, [], "") }
    }
    InitReboot => {
      guard let offer = machine.offer else { return begin_discovery(machine, now) }
      if retry_due(machine, now) { on_tick_reboot(machine, offer, now) } else { step_of(machine, [], "") }
    }
    Probing => {
      guard let candidate = machine.candidate else { return begin_discovery(machine, now) }
      if retry_due(machine, now) { on_tick_probing(machine, candidate, now) } else { step_of(machine, [], "") }
    }
    Bound => {
      guard let lease = held else { return begin_discovery(machine, now) }
      on_tick_bound(machine, lease, now)
    }
    Renewing => {
      guard let lease = held else { return begin_discovery(machine, now) }
      on_tick_renewing(machine, lease, now)
    }
    Rebinding => {
      guard let lease = held else { return begin_discovery(machine, now) }
      on_tick_rebinding(machine, lease, now)
    }
    AdminDown => step_of(machine, [], "")
    NoLink => step_of(machine, [], "")
  }
}

pure expected_xid(machine: Machine, message: wire.Message) -> Bool {
  message.xid == machine.xid
}

pure unexpected(machine: Machine, message: wire.Message) -> Step {
  let kind = if message.kind == wire.OFFER {
    "offer"
  } else if message.kind == wire.ACK {
    "ack"
  } else if message.kind == wire.NAK {
    "nak"
  } else {
    "other"
  }
  step_of(machine, [], f"ignored_{kind}_in_{state_name(machine.state)}")
}

pure accept_ack(machine: Machine, message: wire.Message, now: Int) -> Step {
  if let Ok(lease) = lease_from_ack(message, machine.request_sent_at, machine.timing) {
    accept_lease(machine, lease, now)
  } else {
    restart_after_rejection(machine, now)
  }
}

pure restart_after_rejection(machine: Machine, now: Int) -> Step {
  begin_wait(machine, machine.timing.nak_delay_ms, now, "ack_unusable")
}

pure on_received_selecting(machine: Machine, message: wire.Message, now: Int) -> Step {
  return unexpected(machine, message) unless message.kind == wire.OFFER
  let server = message.server_id ?? 0
  let offer = {address: message.yiaddr, server: server}
  let identified = {...machine, state: Requesting, offer: offer, attempt: 0, request_sent_at: now}
  let waiting = with_retry(backoff(identified, 0), now)
  step_of(waiting, [Send(packet(waiting, RequestSelecting, now, offer.address, offer.server, false))], "")
}

pure on_received_requesting(machine: Machine, message: wire.Message, now: Int) -> Step {
  guard let offer = machine.offer else { return begin_discovery(machine, now) }
  if message.kind == wire.ACK {
    return unexpected(machine, message) unless message.yiaddr == offer.address
    return accept_ack(machine, message, now)
  }
  if message.kind == wire.NAK {
    return unexpected(machine, message) unless message.server_id == offer.server
    return restart_after(machine, NakReceived, machine.timing.nak_delay_ms, now, "nak")
  }
  unexpected(machine, message)
}

pure on_received_reboot(machine: Machine, message: wire.Message, now: Int) -> Step {
  guard let offer = machine.offer else { return begin_discovery(machine, now) }
  if message.kind == wire.ACK {
    return unexpected(machine, message) unless message.yiaddr == offer.address
    return accept_ack(machine, message, now)
  }
  if message.kind == wire.NAK {
    return restart_after(machine, NakReceived, machine.timing.nak_delay_ms, now, "nak")
  }
  unexpected(machine, message)
}

pure on_received_bound(machine: Machine, message: wire.Message, now: Int) -> Step {
  guard let lease = machine.lease else { return unexpected(machine, message) }
  if message.kind == wire.ACK {
    return step_of(machine, [], "ignored_address_change") unless message.yiaddr == lease.address
    return accept_ack(machine, message, now)
  }
  if message.kind == wire.NAK {
    if machine.state == Renewing and message.server_id != lease.server {
      return unexpected(machine, message)
    }
    return restart_after(machine, NakReceived, machine.timing.nak_delay_ms, now, "nak")
  }
  unexpected(machine, message)
}

pure on_received(machine: Machine, message: wire.Message, now: Int) -> Step {
  return step_of(machine, [], "ignored_stale_xid") unless expected_xid(machine, message)
  match machine.state {
    Selecting => on_received_selecting(machine, message, now)
    Requesting => on_received_requesting(machine, message, now)
    InitReboot => on_received_reboot(machine, message, now)
    Renewing => on_received_bound(machine, message, now)
    Rebinding => on_received_bound(machine, message, now)
    Init => unexpected(machine, message)
    Probing => unexpected(machine, message)
    Bound => unexpected(machine, message)
    Declined => unexpected(machine, message)
    AdminDown => unexpected(machine, message)
    NoLink => unexpected(machine, message)
  }
}

pure on_start(machine: Machine, info: StartInfo, now: Int) -> Step {
  let seeded = {...machine, rng: info.seed % 2147483647 + 1, carrier: info.carrier, last_now: now}
  let remembered = info.remembered
  let held = if info.configured { remembered } else { null }
  let loaded = {...seeded, lease: held}
  return step_of({...loaded, state: NoLink, retry_at: null}, [], "") unless info.carrier
  guard let lease = remembered else { return begin_discovery(loaded, now) }
  if lease_due(loaded, now) {
    let dropped = drop_lease(loaded, LeaseExpired)
    let rebooting = begin_reboot(dropped.machine, offer_of(lease), now)
    return step_of(rebooting.machine, dropped.effects.extend(rebooting.effects), "lease_expired")
  }
  begin_reboot(loaded, offer_of(lease), now)
}

pure on_link_down(machine: Machine) -> Step {
  return step_of(machine, [], "ignored_link_down") when machine.state == AdminDown or machine.state == NoLink
  step_of({...machine, state: NoLink, carrier: false, candidate: null, retry_at: null, probes_sent: 0, announces_left: 0}, [], "")
}

pure on_link_up(machine: Machine, recreated: Bool, now: Int) -> Step {
  return step_of(machine, [], "ignored_link_up") unless machine.state == NoLink
  let carried = {...machine, carrier: true}
  guard let lease = carried.lease else {
    return begin_discovery(carried, now)
  }
  if recreated {
    let dropped = drop_lease(carried, LinkRecreated)
    let rebooting = begin_reboot(dropped.machine, offer_of(lease), now)
    return step_of(rebooting.machine, dropped.effects.extend(rebooting.effects), "link_recreated")
  }
  begin_reboot(carried, offer_of(lease), now)
}

pure on_arp_conflict(machine: Machine, now: Int) -> Step {
  guard machine.state == Probing else { return step_of(machine, [], "ignored_arp_conflict") }
  guard let candidate = machine.candidate else { return begin_discovery(machine, now) }
  let declined = {...machine, state: Declined, candidate: null, probes_sent: 0, retry_at: add_ms(now, machine.timing.decline_delay_ms)}
  step_of(declined, [Send(packet(declined, Decline, now, candidate.address, candidate.server, false))], "arp_conflict")
}

pure on_applied(machine: Machine, ok: Bool, now: Int) -> Step {
  return step_of(machine, [], "") when ok
  restart_after(machine, ApplyFailure, machine.timing.nak_delay_ms, now, "apply_failed")
}

pure release_effects(machine: Machine, now: Int) -> Dropped {
  guard let lease = machine.lease else { return {machine: machine, effects: []} }
  let release = Send(packet(machine, Release, now, lease.address, lease.server, true))
  {machine: {...machine, lease: null}, effects: [release, Deconfigure(AdminRequest), Forget]}
}

pure on_admin_down(machine: Machine, now: Int) -> Step {
  let released = release_effects(machine, now)
  let down = {...released.machine, state: AdminDown, offer: null, candidate: null, retry_at: null, probes_sent: 0, announces_left: 0}
  step_of(down, released.effects, "")
}

pure on_admin_up(machine: Machine, now: Int) -> Step {
  return step_of(machine, [], "ignored_admin_up") unless machine.state == AdminDown
  return step_of({...machine, state: NoLink}, [], "") unless machine.carrier
  begin_discovery(machine, now)
}

pure on_shutdown(machine: Machine, release: Bool, now: Int) -> Step {
  return on_admin_down(machine, now) when release
  guard let lease = machine.lease else { return step_of(machine, [], "") }
  step_of(machine, [Persist(lease)], "")
}

pure shift_ms(value: Int?, delta: Int) -> Int? {
  guard let present = value else { return null }
  present - delta
}

pure shifted_lease(lease: Lease?, delta: Int) -> Lease? {
  guard let present = lease else { return null }
  {...present, acquired_at: present.acquired_at - delta, renew_at: shift_ms(present.renew_at, delta), rebind_at: shift_ms(present.rebind_at, delta), expire_at: shift_ms(present.expire_at, delta)}
}

pure rebase_regression(machine: Machine, now: Int) -> Machine {
  let delta = machine.last_now - now
  let bound_like = machine.state == Bound or machine.state == Renewing or machine.state == Rebinding
  let state = if bound_like and machine.lease != null { InitReboot } else { machine.state }
  let offer = if bound_like and machine.lease != null { offer_of_held(machine) } else { machine.offer }
  {...machine, state: state, offer: offer, lease: shifted_lease(machine.lease, delta), candidate: shifted_lease(machine.candidate, delta), retry_at: shift_ms(machine.retry_at, delta), began_at: machine.began_at - delta, request_sent_at: machine.request_sent_at - delta}
}

pure offer_of_held(machine: Machine) -> Offer? {
  guard let lease = machine.lease else { return machine.offer }
  offer_of(lease)
}

pure revalidate_after_regression(machine: Machine, now: Int) -> Step {
  guard let offer = machine.offer else { return begin_discovery(machine, now) }
  if machine.state == InitReboot {
    let rebooting = begin_reboot(machine, offer, now)
    return step_of(rebooting.machine, rebooting.effects, "clock_regression")
  }
  step_of(machine, [], "clock_regression")
}

pure wake_candidates(machine: Machine) -> List[Int] {
  let {state, lease, ..} = machine
  let renewing = if state == Bound { lease?.renew_at } else { null }
  let rebinding = if state == Bound or state == Renewing { lease?.rebind_at } else { null }
  collect {
    for candidate in [machine.retry_at, lease?.expire_at, renewing, rebinding] {
      if let moment = candidate {
        yield moment
      }
    }
  }
}

pure earliest(times: List[Int]) -> Int? {
  guard ! times.is_empty() else { return null }
  var best = times[0]
  for moment in times {
    if moment < best { best = moment }
  }
  best
}

pure wake_effect(before: Int?, after: Int?) -> List[Effect] {
  return [] when before == after
  guard let at = after else { return [Disarm] }
  [Arm(at)]
}

pure finish(before: Machine, after: Step, now: Int) -> Step {
  let wake = earliest(wake_candidates(after.machine))
  let settled = {...after.machine, wake_at: wake, last_now: now}
  var effects = after.effects.extend(wake_effect(before.wake_at, wake))
  if status(before) != status(settled) {
    effects += [Publish(status(settled))]
  }
  step_of(settled, effects, after.note)
}

pure dispatch(machine: Machine, event: Event, now: Int) -> Step {
  match event {
    OnStart(info) => on_start(machine, info, now)
    OnReceived(message) => on_received(machine, message, now)
    OnTick => on_tick(machine, now)
    OnLinkUp(recreated) => on_link_up(machine, recreated, now)
    OnLinkDown => on_link_down(machine)
    OnArpConflict => on_arp_conflict(machine, now)
    OnApplied(ok) => on_applied(machine, ok, now)
    OnAdminUp => on_admin_up(machine, now)
    OnAdminDown => on_admin_down(machine, now)
    OnShutdown(release) => on_shutdown(machine, release, now)
  }
}

pure starts(event: Event) -> Bool {
  match event {
    OnStart(_) => true
    else => false
  }
}

pure settle_time(machine: Machine, event: Event, now: Int) -> Step {
  return step_of(machine, [], "") when starts(event) or now >= machine.last_now
  let rebased = rebase_regression(machine, now)
  revalidate_after_regression(rebased, now)
}

## The next machine and effects for one event at time `now`. A clock reading
## that moves backwards (a restored snapshot) keeps every remaining duration,
## and a bound lease is revalidated with a request for its address. A lease
## whose end has passed is withdrawn before the event is handled, so a reply
## that arrives late never extends a lease that already ended.
export pure transition(machine: Machine, event: Event, now: Int) -> Step {
  let settled = settle_time(machine, event, now)
  let aged = if starts(event) or ! lease_due(settled.machine, now) {
    step_of(settled.machine, settled.effects, settled.note)
  } else {
    let ended = expire(settled.machine, now)
    step_of(ended.machine, settled.effects.extend(ended.effects), ended.note)
  }
  let handled = dispatch(aged.machine, event, now)
  let effects = aged.effects.extend(handled.effects)
  let note = if handled.note == "" { aged.note } else { handled.note }
  finish(machine, step_of(handled.machine, effects, note), now)
}
