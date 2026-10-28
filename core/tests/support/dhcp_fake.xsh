##! Test support: a scripted world for the client loop. Time is virtual and
##! advances only when the loop waits, a scripted server answers every message
##! the client sends by rule, and a modelled interface applies the client's
##! network changes and can be made to fail, lose its address, lose its
##! carrier or be recreated at chosen moments. Resolver publication and lease
##! records are real files in a test directory. The world lives in one JSON
##! file below `Settings.scratch`, because a `ClientIo` is a record of plain
##! procedures with no state of their own. Nothing here is reachable from the
##! production client: only a test constructs the settings that name it.
use lib.dhcp_client as client
use lib.dhcp_dns as dns_owner
use lib.dhcp_machine as machine
use lib.dhcp_net as network
use lib.dhcp_store as store
use lib.dhcp_wire as wire
use tests.support.dhcp_kernel as kernel_model
use tests.support.dhcp_pcap as pcap
use tests.support.dhcp_responder as responder
use tests.support.dhcp_types as types

## One scripted happening. `kind` is one of: `frame` (deliver the base64 datagram
## in `text`), `arp` (deliver an ARP frame), `lost` (deliver a notification
## overrun), `changed` (deliver a link change), `ask` (a command: `stop`,
## `down` or `up`), `carrier` (`up` or `down`), `recreate` (a new index),
## `unplug_address` (remove the addresses behind the client's back),
## `fail_next` (fail the next network operation), `jump` (add `text`
## milliseconds to the boot clock without delivering anything in between) and
## `wall` (add `text` milliseconds to the wall clock) and `later` (`text` is
## `delay|kind|text`: schedule that happening `delay` milliseconds after the
## boot clock as it reads when this one is delivered).
export type Happening = {at: Int, kind: Str, text: Str}

## What the client sent.
export type Sent = {at: Int, kind: Str, destination: Int, message: Int, xid: Int}

type KernelJson = {
  ifindex: Int,
  name: Str,
  mac: Str,
  up: Bool,
  carrier: Bool,
  addresses: List[network.Address],
  routes: List[network.Route],
  fail_at: Int,
  calls: Int,
}

## Everything the fake world holds.
export type World = {
  now: Int,
  wall: Int,
  seed: Int,
  rules: List[types.Rule],
  counts: responder.Counts,
  inbox: List[Happening],
  sent: List[Sent],
  kernel: KernelJson,
  conflicting: Int?,
  waits: Int,
  persisted: Int,
}

const SERVER_MAC = b"\x02\x00\x00\x00\x00\x02"
const SERVER_ADDRESS = 3232248321

pure unhex(text: Str) -> Bytes {
  let values = [("0123456789abcdef".find(text.byte_slice(index * 2, length: 1)) ?? 0) * 16 + ("0123456789abcdef".find(text.byte_slice(index * 2 + 1, length: 1)) ?? 0) for index in range(text.byte_len() / 2)]
  bytes.from_ints(values) ?? b""
}

pure hex(data: Bytes) -> Str {
  var text = ""
  for index in range(data.len()) {
    let value = data.byte_at(index) ?? 0
    text += "0123456789abcdef".byte_slice(value / 16, length: 1) + "0123456789abcdef".byte_slice(value % 16, length: 1)
  }
  text
}

pure world_path(settings: client.Settings) -> Path {
  fp"{settings.scratch ?? p"."}/world.json"
}

## Reads the world of a client's settings.
export proc load(settings: client.Settings) [fs, error] -> Result[World, Error] {
  json.read(world_path(settings))?.require()
}

## Writes the world.
export proc save(settings: client.Settings, world: World) [fs, error] {
  world_path(settings).write(json.encode(world)?)
}

## A world at time `start` with the interface `name` (index 3, address-less,
## up, with carrier), a server following `rules`, and `inbox` scripted.
export pure world_of(link: client.Link, start: Int, rules: List[types.Rule], inbox: List[Happening]) -> World {
  World(
    now: start,
    wall: 1700000000000 + start,
    seed: 4242,
    rules:,
    counts: responder.Counts(discover: 0, request: 0, decline: 0, release: 0, inform: 0),
    inbox:,
    sent: [],
    kernel: KernelJson(ifindex: link.ifindex, name: link.name, mac: hex(link.mac), up: true, carrier: true, addresses: [], routes: [], fail_at: -1, calls: 0),
    conflicting: null,
    waits: 0,
    persisted: 0,
  )
}

pure observation_of(kernel: KernelJson) -> network.Observation {
  {ifindex: kernel.ifindex, name: kernel.name, mac: unhex(kernel.mac), up: kernel.up, carrier: kernel.carrier, addresses: kernel.addresses, routes: kernel.routes}
}

## The observation the world's interface currently has.
export pure observed_in(world: World) -> network.Observation {
  observation_of(world.kernel)
}

pure earliest_at(world: World) -> Int? {
  guard ! world.inbox.is_empty() else { return null }
  var best = world.inbox[0].at
  for item in world.inbox {
    best = item.at when item.at < best
  }
  best
}

pure due(world: World) -> List[Happening] {
  [item for item in world.inbox if item.at <= world.now] |> sort-by .at
}

pure pending(world: World) -> List[Happening] {
  [item for item in world.inbox if item.at > world.now]
}

proc fake_now(ctx: client.Context) [fs, time, error] -> Result[Int, Error] {
  Ok(load(ctx.settings)?.now)
}

proc fake_wall(ctx: client.Context) [fs, time, error] -> Result[Int, Error] {
  Ok(load(ctx.settings)?.wall)
}

proc fake_random(ctx: client.Context) [fs, error] -> Result[Int, Error] {
  Ok(load(ctx.settings)?.seed)
}

pure target_time(now: Int, limit: Int?, next: Int?) -> Int {
  guard let boundary = limit else { return next ?? now }
  guard let upcoming = next else { return boundary }
  let earlier = if upcoming < boundary { upcoming } else { boundary }
  if earlier > now { earlier } else { now }
}

pure readiness(due_now: List[Happening]) -> client.Ready {
  client.Ready(
    dhcp: has_kind(due_now, ["frame"]),
    arp: has_kind(due_now, ["arp"]),
    events: has_kind(due_now, ["changed", "lost", "carrier", "recreate", "unplug_address", "fail_next", "jump", "wall", "later"]),
    control: has_kind(due_now, ["ask"]),
  )
}

proc fake_wait(ctx: client.Context, timeout_ms: Int?) [fs, process, net, time, error] -> Result[client.Ready, Error] {
  let world = load(ctx.settings)?
  let limit = if let pause = timeout_ms { world.now + pause } else { null }
  let next = earliest_at(world)
  if limit == null and next == null {
    return Err(error.failure("the fake world would wait forever: nothing is scheduled"))
  }
  let moved = {...world, now: target_time(world.now, limit, next), waits: world.waits + 1}
  save(ctx.settings, moved)
  Ok(readiness(due(moved)))
}

pure has_kind(items: List[Happening], kinds: List[Str]) -> Bool {
  for item in items {
    return true when item.kind in kinds
  }
  false
}

pure with_kernel(world: World, kernel: KernelJson) -> World {
  {...world, kernel: kernel}
}

pure ask_request(text: Str) -> client.Request {
  match text {
    "down" => client.AdminDown
    "up" => client.AdminUp
    else => client.Stop
  }
}

proc fake_receive(ctx: client.Context) [fs, process, net, error] -> Result[List[client.Incoming], Error] {
  var world = load(ctx.settings)?
  var remaining = pending(world)
  var delivered = []
  for item in due(world) {
    match item.kind {
      "frame" => delivered += [client.GotFrame(unhex(item.text))]
      "arp" => delivered += [.GotArp(unhex(item.text))]
      "lost" => delivered += [client.LinkLost]
      "changed" => delivered += [client.LinkChanged]
      "ask" => delivered += [.Asked(ask_request(item.text))]
      "carrier" => {
        world = with_kernel(world, {...world.kernel, carrier: item.text == "up"})
        delivered += [client.LinkChanged]
      }
      "recreate" => {
        world = with_kernel(world, {...world.kernel, ifindex: world.kernel.ifindex + 10, addresses: [], routes: []})
        delivered += [client.LinkChanged]
      }
      "unplug_address" => {
        world = with_kernel(world, {...world.kernel, addresses: [], routes: []})
        delivered += [client.LinkLost]
      }
      "fail_next" => world = with_kernel(world, {...world.kernel, fail_at: world.kernel.calls})
      "jump" => world = {...world, now: world.now + item.text as Int}
      "wall" => world = {...world, wall: world.wall + item.text as Int}
      "later" => {
        let parts = item.text.split("|", maxsplit: 2)
        remaining += [{at: world.now + parts[0] as Int, kind: parts[1], text: parts[2]}]
      }
      else => {}
    }
  }
  save(ctx.settings, {...world, inbox: remaining})
  Ok(delivered)
}

pure reply_frames(world: World, request: wire.Message, link: client.Link) -> List[Happening] {
  let kind = responder.kind_name(request.kind)
  let counts = responder.bump(world.counts, kind)
  let occurrence = responder.seen(counts, kind)
  guard let rule = responder.pick(world.rules, kind, occurrence) else { return [] }
  collect {
    for reply in rule.replies {
      let datagram = responder.reply_datagram(reply, request)
      let broadcast = reply.dest == "broadcast"
      let destination = if broadcast { b"\xff\xff\xff\xff\xff\xff" } else { link.mac }
      let target = if broadcast { 4294967295 } else { wire.ip_parse(responder.destination(reply, responder.Request(kind:, message: request, source: wire.ip_text(request.ciaddr), source_port: 68))) ?? 4294967295 }
      repeat max_of(reply.repeat, 1) times {
        yield {at: world.now + reply.delay_ms, kind: "frame", text: hex(pcap.udp_frame(destination, SERVER_MAC, SERVER_ADDRESS, target, 67, 68, datagram))}
      }
    }
  }
}

pure max_of(left: Int, right: Int) -> Int {
  if left > right { left } else { right }
}

proc fake_send(ctx: client.Context, outgoing: client.Outgoing) [fs, process, net, error] {
  var world = load(ctx.settings)?
  match outgoing {
    client.SendDhcp(found) => {
      let message = wire.decode(found.bytes)?
      let counted = responder.bump(world.counts, responder.kind_name(message.kind))
      let replies = reply_frames(world, message, ctx.link)
      world = {...world, counts: counted, inbox: world.inbox.extend(replies), sent: world.sent.push({at: world.now, kind: "dhcp", destination: found.destination, message: message.kind, xid: message.xid})}
    }
    client.SendArp(frame) => {
      let probed = arp_target(frame)
      world = {...world, sent: world.sent.push({at: world.now, kind: "arp", destination: probed, message: 0, xid: 0})}
      if world.conflicting == probed and probed != 0 {
        let claim = pcap_arp_claim(probed)
        world = {...world, inbox: world.inbox.push({at: world.now, kind: "arp", text: hex(claim)})}
      }
    }
  }
  save(ctx.settings, world)
}

pure arp_target(frame: Bytes) -> Int {
  bytes.unpack_be(frame, 4, 38) ?? 0
}

pure pcap_arp_claim(address: Int) -> Bytes {
  let body = bytes.concat([b"\xff\xff\xff\xff\xff\xff", b"\x02\xee\xee\xee\xee\xee", b"\x08\x06", b"\x00\x01\x08\x00\x06\x04\x00\x02", b"\x02\xee\xee\xee\xee\xee", bytes.pack_be(address, 4) ?? b"", bytes.zero(6) ?? b"", bytes.pack_be(address, 4) ?? b""])
  bytes.concat([body, bytes.zero(60 - body.len()) ?? b""])
}

proc fake_observe(ctx: client.Context) [fs, process, net, error] -> Result[network.Observation, Error] {
  Ok(observed_in(load(ctx.settings)?))
}

pure injected_failure_spent(fail_at: Int, outcome: network.Outcome?) -> Int {
  guard let finished = outcome else { return fail_at }
  if let .Partial(_) = finished.settled { -1 } else { fail_at }
}

proc fake_apply(ctx: client.Context, change: client.Change) [fs, process, net, time, error] -> Result[client.Applied, Error] {
  var world = load(ctx.settings)?
  match change {
    client.Reconcile(request) => {
      let model = kernel_model.Kernel(observed: observation_of(world.kernel), fail_at: world.kernel.fail_at, calls: world.kernel.calls)
      let played = kernel_model.play(request.desired, request.metric, request.record.owned, model, false)
      let state = played.kernel.observed
      let reached = KernelJson(
        ifindex: world.kernel.ifindex,
        name: world.kernel.name,
        mac: world.kernel.mac,
        up: state.up,
        carrier: world.kernel.carrier,
        addresses: state.addresses,
        routes: state.routes,
        fail_at: injected_failure_spent(world.kernel.fail_at, played.outcome),
        calls: played.kernel.calls,
      )
      save(ctx.settings, {...world, kernel: reached})
      if let outcome = played.outcome {
        store.save(fs.open_root(ctx.settings.lease_dir)?, {...request.record, owned: outcome.owned, reconcile: false})
        return Ok(.Reconciled(outcome))
      }
      Err(error.failure("the modelled reconciliation did not finish"))
    }
    client.Dns(contribution) => {
      let layout = dns_owner.Layout(run_dir: ctx.settings.run_dir, destination_dir: ctx.settings.resolv_dir, destination: ctx.settings.resolv_name, static_path: ctx.settings.static_path, lock_wait_ms: 1000)
      Ok(.Resolver(dns_owner.update(layout, ctx.settings.interface, contribution)?))
    }
    client.Persist(record) => {
      store.save(fs.open_root(ctx.settings.lease_dir)?, record)
      save(ctx.settings, {...world, persisted: world.persisted + 1})
      Ok(client.Done)
    }
    client.Publish(text) => {
      fp"{ctx.settings.run_dir}/{ctx.settings.interface}.status".write_atomic(text)
      Ok(client.Done)
    }
  }
}

proc fake_rebind(ctx: client.Context, observed: network.Observation) [fs, process, net, time, error] -> Result[client.Context, Error] {
  Ok({...ctx, link.ifindex: observed.ifindex})
}

## The `ClientIo` over the fake world.
export pure scripted_io() -> client.ClientIo {
  client.ClientIo(
    now: fake_now,
    wall: fake_wall,
    random: fake_random,
    wait: fake_wait,
    receive: fake_receive,
    send: fake_send,
    observe: fake_observe,
    apply: fake_apply,
    rebind: fake_rebind,
  )
}
