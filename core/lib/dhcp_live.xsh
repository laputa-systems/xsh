##! The production `ClientIo` and the sockets it uses: a packet socket that
##! receives DHCP frames whether or not the interface has an address, a UDP
##! socket that sends, a packet socket for ARP, a route-netlink socket for link
##! and address events, and a Unix datagram socket the operator's `ifup` and
##! `ifdown` write to. One `unix.poll_fds` waits on all of them and on the
##! machine's next deadline, so an idle client makes no wakeups. The boot clock
##! keeps counting through suspend; the wall clock is read only for display.
use dhcp_apply as apply
use dhcp_arp as arp
use dhcp_client as client
use dhcp_dns as dns_owner
use dhcp_frame as frame_codec
use dhcp_machine as machine
use dhcp_net as network
use dhcp_store as store
use dhcp_wire as wire

const MAX_FRAMES = 32
const MAX_EVENT_READS = 16
const FRAME_BYTES = 2048
const CONTROL_BYTES = 64
const ENOBUFS = 105
const EAGAIN_CODES = [11, 35]

## The identity of the running boot, used to tell a remembered lease from a
## stale one.
export proc boot_identity() [fs, error] -> Result[Str, Error] {
  Ok(p"/proc/sys/kernel/random/boot_id".read_text()?.trim())
}

## The facts of the interface `name`: its index, its hardware address and the
## DHCP identity derived from it.
export proc resolve(name: Str) [process, net, error] -> Result[client.Link, Error] {
  let observed = apply.observe(name)?
  let identity = wire.identity(observed.mac)?
  Ok(client.Link(name, observed.ifindex, observed.mac, identity))
}

proc udp_socket(link: client.Link) [process, net, error] -> Result[Int, Error] {
  let c = linux.net_constants()
  let socket = linux.socket(c.AF_INET, c.SOCK_DGRAM)?
  linux.setsockopt_int(socket, c.SOL_SOCKET, c.SO_REUSEADDR, 1)
  linux.setsockopt_int(socket, c.SOL_SOCKET, c.SO_BROADCAST, 1)
  linux.setsockopt_bytes(socket, c.SOL_SOCKET, c.SO_BINDTODEVICE, bytes.from_text(link.name))
  linux.bind(socket, {family: "inet", address: "0.0.0.0", port: wire.CLIENT_PORT})
  Ok(socket)
}

proc packet_socket(link: client.Link, protocol: Int) [process, net, error] -> Result[Int, Error] {
  let c = linux.net_constants()
  let network_order = protocol % 256 * 256 + protocol / 256
  let socket = linux.socket(c.AF_PACKET, c.SOCK_RAW.bit_or(c.SOCK_NONBLOCK), network_order)?
  linux.bind(socket, arp.packet_address(link.ifindex, protocol, b"")?)
  Ok(socket)
}

proc control_socket(settings: client.Settings) [fs, process, net, error] -> Result[Int, Error] {
  let c = linux.net_constants()
  let socket_file = fp"{settings.run_dir}/{settings.interface}.control"
  if socket_file.exists() {
    socket_file.remove()
  }
  let socket = linux.socket(c.AF_UNIX, c.SOCK_DGRAM.bit_or(c.SOCK_NONBLOCK))?
  let directory = unix.open_fd(settings.run_dir, flags: ["directory"])?
  defer unix.close_fd(directory)
  linux.bind(socket, {family: "unix", address: f"/proc/self/fd/{directory}/{settings.interface}.control"})
  socket_file.chmod(0o600)
  Ok(socket)
}

proc event_socket() [process, net, error] -> Result[Int, Error] {
  let c = linux.net_constants()
  let groups = c.RTMGRP_LINK.bit_or(c.RTMGRP_IPV4_IFADDR).bit_or(c.RTMGRP_IPV4_ROUTE)
  linux.netlink_open(c.NETLINK_ROUTE, groups)
}

## Opens every socket the client uses. The caller holds the interface lock, so
## a control socket file that exists is stale and is replaced.
export proc open_sockets(settings: client.Settings, link: client.Link) [fs, process, net, error] -> Result[client.Sockets, Error] {
  let c = linux.net_constants()
  Ok(
    client.Sockets(
      udp: udp_socket(link)?,
      packets: packet_socket(link, c.ETH_P_IP)?,
      arp: packet_socket(link, c.ETH_P_ARP)?,
      events: event_socket()?,
      control: control_socket(settings)?,
    ),
  )
}

## Closes the sockets and removes the control socket file.
export proc close_sockets(settings: client.Settings, sockets: client.Sockets) [fs, process, error] {
  for descriptor in [sockets.udp, sockets.packets, sockets.arp, sockets.events, sockets.control] {
    let _ = unix.close_fd(descriptor)
  }
  let socket_file = fp"{settings.run_dir}/{settings.interface}.control"
  if socket_file.exists() {
    socket_file.remove()
  }
}

proc live_now(ctx: client.Context) [fs, process, net, time, error] -> Result[Int, Error] {
  ctx f"reading the boot clock for {ctx.settings.interface}" {
    Ok(time.boottime()? / 1000000)
  }
}

proc live_wall(ctx: client.Context) [fs, process, net, time, error] -> Result[Int, Error] {
  ctx f"reading the wall clock for {ctx.settings.interface}" {
    Ok(time.now())
  }
}

proc live_random(ctx: client.Context) [fs, process, net, time, error] -> Result[Int, Error] {
  let source = unix.open_fd(p"/dev/urandom")?
  defer unix.close_fd(source)
  let entropy = bytes.unpack_be(unix.read_fd(source, 4)?, 4)?
  let hardware = bytes.unpack_be(ctx.link.mac, 4, 2)?
  Ok((entropy + hardware) % 2147483647)
}

proc live_wait(ctx: client.Context, timeout_ms: Int?) [fs, process, net, time, error] -> Result[client.Ready, Error] {
  let sockets = ctx.sockets
  let entries = [
    {fd: sockets.packets, events: ["readable"]},
    {fd: sockets.arp, events: ["readable"]},
    {fd: sockets.events, events: ["readable"]},
    {fd: sockets.control, events: ["readable"]},
  ]
  let ready = unix.poll_fds(entries, if let limit = timeout_ms { limit * 1ms } else { null })?
  let fds = [item.fd for item in ready]
  Ok(client.Ready(dhcp: sockets.packets in fds, arp: sockets.arp in fds, events: sockets.events in fds, control: sockets.control in fds))
}

proc drain_frames(descriptor: Int) [process, net, error] -> Result[List[Bytes], Error] {
  var frames = []
  while frames.len() < MAX_FRAMES and unix.poll_fd(descriptor, ["readable"], 0)? == ["readable"] {
    match unix.read_fd(descriptor, FRAME_BYTES) {
      Ok(frame) => frames += [frame]
      Err(failure) => return Err(failure) when (failure.errno ?? 0) not in EAGAIN_CODES
    }
  }
  Ok(frames)
}

proc drain_events(descriptor: Int) [process, net, error] -> Result[List[client.Incoming], Error] {
  var seen = []
  var reads = 0
  while reads < MAX_EVENT_READS and unix.poll_fd(descriptor, ["readable"], 0)? == ["readable"] {
    reads += 1
    match unix.read_fd(descriptor, 65536) {
      Ok(_) => {
        if client.LinkChanged not in seen {
          seen += [client.LinkChanged]
        }
      }
      Err(failure) => {
        return Ok([client.LinkLost]) when (failure.errno ?? 0) == ENOBUFS
        return Err(failure) when (failure.errno ?? 0) not in EAGAIN_CODES
      }
    }
  }
  Ok(seen)
}

proc drain_control(descriptor: Int) [process, net, error] -> Result[List[client.Incoming], Error] {
  var reads = 0
  let asked = collect {
    while reads < MAX_EVENT_READS and unix.poll_fd(descriptor, ["readable"], 0)? == ["readable"] {
      reads += 1
      let received = linux.recvfrom(descriptor, CONTROL_BYTES)?
      match received.data.utf8() ?? "" {
        "down" => yield client.Asked(client.AdminDown)
        "up" => yield client.Asked(client.AdminUp)
        "stop" => yield client.Asked(client.Stop)
        else => {}
      }
    }
  }
  Ok(asked)
}

proc live_receive(ctx: client.Context) [fs, process, net, error] -> Result[List[client.Incoming], Error] {
  let sockets = ctx.sockets
  let frames = [client.GotFrame(frame) for frame in drain_frames(sockets.packets)?]
  let probes = [client.GotArp(frame) for frame in drain_frames(sockets.arp)?]
  Ok([@frames, @probes, @drain_events(sockets.events)?, @drain_control(sockets.control)?])
}

proc live_send(ctx: client.Context, outgoing: client.Outgoing) [fs, process, net, error] -> Result[Unit, Error] {
  let c = linux.net_constants()
  match outgoing {
    client.SendDhcp(found) => {
      if found.unaddressed {
        let frame = frame_codec.udp_frame(b"\xff\xff\xff\xff\xff\xff", ctx.link.mac, 0, 4294967295, wire.CLIENT_PORT, wire.SERVER_PORT, found.bytes)
        let _ = linux.sendto(ctx.sockets.packets, frame, arp.packet_address(ctx.link.ifindex, c.ETH_P_IP, b"\xff\xff\xff\xff\xff\xff")?)?
        return
      }
      let target = if found.broadcast { "255.255.255.255" } else { wire.ip_text(found.destination) }
      let _ = linux.sendto(ctx.sockets.udp, found.bytes, {family: "inet", address: target, port: wire.SERVER_PORT})?
      Ok()
    }
    client.SendArp(frame) => {
      let destination = arp.packet_address(ctx.link.ifindex, c.ETH_P_ARP, b"\xff\xff\xff\xff\xff\xff")?
      let _ = linux.sendto(ctx.sockets.arp, frame, destination)?
      Ok()
    }
  }
}

proc live_observe(ctx: client.Context) [fs, process, net, error] -> Result[network.Observation, Error] {
  apply.observe(ctx.link.name)
}

proc live_apply(ctx: client.Context, change: client.Change) [fs, process, net, time, error] -> Result[client.Applied, Error] {
  let settings = ctx.settings
  match change {
    client.Reconcile(request) => {
      let root = fs.open_root(settings.lease_dir)?
      Ok(.Reconciled(apply.reconcile(root, request.record, request.desired, request.metric)?))
    }
    client.Dns(contribution) => {
      let layout = dns_owner.Layout(run_dir: settings.run_dir, destination_dir: settings.resolv_dir, destination: settings.resolv_name, static_path: settings.static_path, lock_wait_ms: dns_owner.LOCK_WAIT_MS)
      Ok(.Resolver(dns_owner.update(layout, settings.interface, contribution)?))
    }
    client.Persist(record) => {
      store.save(fs.open_root(settings.lease_dir)?, record)
      Ok(client.Done)
    }
    client.Publish(text) => {
      let root = fs.open_root(settings.run_dir)?
      root.write_atomic(fp"{settings.interface}.status".require()?, text, mode: 0o644, replace: "regular_or_absent")
      Ok(client.Done)
    }
  }
}

proc live_rebind(ctx: client.Context, observed: network.Observation) [fs, process, net, time, error] -> Result[client.Context, Error] {
  let c = linux.net_constants()
  let link = {...ctx.link, ifindex: observed.ifindex}
  let _ = unix.close_fd(ctx.sockets.packets)
  let _ = unix.close_fd(ctx.sockets.arp)
  let sockets = {...ctx.sockets, packets: packet_socket(link, c.ETH_P_IP)?, arp: packet_socket(link, c.ETH_P_ARP)?}
  Ok({...ctx, link: link, sockets: sockets})
}

## The production host operations.
export pure host() -> client.ClientIo {
  client.ClientIo(
    now: live_now,
    wall: live_wall,
    random: live_random,
    wait: live_wait,
    receive: live_receive,
    send: live_send,
    observe: live_observe,
    apply: live_apply,
    rebind: live_rebind,
  )
}

## The directories and policy of a production client for `interface`. The lease
## record is kept under `/var/lib/dhcpc`, the lock, status, control socket and
## resolver contributions under `/run/dhcpc`, the resolver file is
## `/etc/resolv.conf` and the operator's static resolver lines come from
## `/etc/dhcpc/resolv.static`.
export pure default_settings(interface: Str) -> client.Settings {
  client.Settings(
    interface:,
    lease_dir: p"/var/lib/dhcpc",
    run_dir: p"/run/dhcpc",
    resolv_dir: p"/etc",
    resolv_name: "resolv.conf",
    static_path: p"/etc/dhcpc/resolv.static",
    metric: network.DEFAULT_METRIC,
    timing: machine.default_timing(),
    scratch: null,
  )
}

## `default_settings` with the lease, run and resolver directories read from
## `XSH_DHCPC_LEASE_DIR`, `XSH_DHCPC_RUN_DIR` and `XSH_DHCPC_RESOLV_DIR` when
## they are set, so `ifup` and `ifdown` can address the directories a client
## was started with. The variables name directories only; they never select a
## transport or a clock.
export proc settings_from_environment(interface: Str) [env, error] -> Result[client.Settings, Error] {
  let base = default_settings(interface)
  Ok(
    {
      ...base,
      lease_dir: fp"{e"XSH_DHCPC_LEASE_DIR" ?? base.lease_dir.display()}",
      run_dir: fp"{e"XSH_DHCPC_RUN_DIR" ?? base.run_dir.display()}",
      resolv_dir: fp"{e"XSH_DHCPC_RESOLV_DIR" ?? base.resolv_dir.display()}",
    },
  )
}

proc ensure_directories(settings: client.Settings) [fs, error] {
  if ! settings.run_dir.exists() {
    settings.run_dir.mkdir()
  }
  if ! settings.lease_dir.exists() {
    settings.lease_dir.mkdir()
    settings.lease_dir.chmod(0o700)
  }
}

## Takes the exclusive lock of the interface without waiting. Null means a
## client (or a one-shot run) already owns the interface.
export proc lock_interface(settings: client.Settings) [fs, error] -> Result[FsLock?, Error] {
  ensure_directories(settings)
  match fs.lock(fp"{settings.run_dir}/{settings.interface}.lock", nonblocking: true) {
    Ok(held) => Ok(held)
    Err(failure) => if (failure.errno ?? 0) in EAGAIN_CODES { Ok(null) } else { Err(failure) }
  }
}

pure admin_mark(settings: client.Settings) -> Path {
  fp"{settings.run_dir}/{settings.interface}.admin-down"
}

## Records whether the operator has taken the interface down. The mark
## survives a client restart so a restart does not undo `ifdown`.
export proc set_admin_down(settings: client.Settings, down: Bool) [fs, error] {
  ensure_directories(settings)
  if down {
    admin_mark(settings).write("down\n")
  } else if admin_mark(settings).exists() {
    admin_mark(settings).remove()
  }
}

## Sends one command ("down", "up" or "stop") to the running client's control
## socket, addressed through a descriptor of the run directory so a long
## directory path never overruns the socket address.
export proc ask(settings: client.Settings, command: Str) [fs, process, net, error] -> Result[Unit, Error] {
  let c = linux.net_constants()
  let socket = linux.socket(c.AF_UNIX, c.SOCK_DGRAM)?
  defer unix.close_fd(socket)
  let directory = unix.open_fd(settings.run_dir, flags: ["directory"])?
  defer unix.close_fd(directory)
  let _ = linux.sendto(socket, bytes.from_text(command), {family: "unix", address: f"/proc/self/fd/{directory}/{settings.interface}.control"})?
  Ok()
}

type PublishedState = {state: Str}

## The state the running client last published, or null when it has not.
export proc published_state(settings: client.Settings) [fs, error] -> Result[Str?, Error] {
  let status = fp"{settings.run_dir}/{settings.interface}.status"
  guard status.exists() else { return Ok(null) }
  Ok(json.decode(status.read_text()?)?.require(PublishedState)?.state)
}

## Waits up to `wait_ms` for the client to publish one of `states`, and returns
## the last state seen (null when none was published).
export proc await_state(settings: client.Settings, states: List[Str], wait_ms: Int) [fs, time, error] -> Result[Str?, Error] {
  var waited = 0
  var seen = null
  while waited <= wait_ms {
    seen = published_state(settings)?
    if let state = seen {
      return Ok(seen) when state in states
    }
    time.sleep(100ms)
    waited += 100
  }
  Ok(seen)
}

proc publish_stopped(settings: client.Settings) [fs, error] {
  let status = fp"{settings.run_dir}/{settings.interface}.status"
  status.write_atomic("{\"state\":\"stopped\"}\n")
}

## Runs the client for the interface in `settings` until stopped (supervised)
## or until a lease is bound (one-shot). The interface lock is held for the
## whole run. `notify` is the supervisor's readiness descriptor: it is written
## once the interface is resolved, the lock is held and the sockets are open.
export proc supervise(settings: client.Settings, mode: client.Mode, notify: Int?) [fs, process, net, time, error] -> Result[client.Session, Error] {
  guard let held = lock_interface(settings)? else {
    return Err(client.ClientFailure.Unreachable(f"another client owns {settings.interface}"))
  }
  defer fs.unlock(held)
  let first = apply.observe(settings.interface)?
  if ! first.up {
    linux.link_up(settings.interface)
  }
  let link = resolve(settings.interface)?
  let boot = boot_identity()?
  let record = store.load(fs.open_root(settings.lease_dir)?, settings.interface)?
  let observed = apply.observe(settings.interface)?
  let sockets = open_sockets(settings, link)?
  defer close_sockets(settings, sockets)
  defer publish_stopped(settings)
  let ctx = client.Context(settings:, link:, sockets:, boot_id: boot)
  let beginning = client.Beginning(record:, observed:, admin_down: admin_mark(settings).exists()?)
  if let descriptor = notify {
    assert unix.write_fd(descriptor, b"r")? == 1
    unix.close_fd(descriptor)?
  }
  client.drive(host(), ctx, beginning, mode)
}

proc send_release(link: client.Link, lease: machine.Lease, xid: Int) [fs, process, net, error] {
  let c = linux.net_constants()
  let socket = udp_socket(link)?
  defer unix.close_fd(socket)
  let datagram = wire.release(link.identity, xid, lease.address, lease.server)?
  let _ = linux.sendto(socket, datagram, {family: "inet", address: wire.ip_text(lease.server), port: wire.SERVER_PORT})?
}

## Releases the lease of an interface that has no running client: sends
## DHCPRELEASE to the server, withdraws the owned addresses and routes, removes
## the interface's resolver contribution and forgets the lease. Nothing else
## on the interface is touched. Returns a one-line description of what was done.
export proc release_offline(settings: client.Settings) [fs, process, net, time, error] -> Result[Str, Error] {
  let link = resolve(settings.interface)?
  let boot = boot_identity()?
  let root = fs.open_root(settings.lease_dir)?
  guard let record = store.load(root, settings.interface)? else { return Ok("no lease record") }
  let revived = store.revalidate(record, store.Current(name: link.name, ifindex: link.ifindex, mac: link.mac, boot_id: boot))
  if let lease = revived.lease {
    if revived.same_boot {
      send_release(link, lease, bytes.unpack_be(unix.read_fd(unix.open_fd(p"/dev/urandom")?, 4)?, 4)?)
    }
  }
  let base = {...record, lease: null, ifindex: link.ifindex, owned: revived.owned, boot_id: boot}
  let outcome = apply.reconcile(root, base, null, settings.metric)?
  let layout = dns_owner.Layout(run_dir: settings.run_dir, destination_dir: settings.resolv_dir, destination: settings.resolv_name, static_path: settings.static_path, lock_wait_ms: dns_owner.LOCK_WAIT_MS)
  let _ = dns_owner.update(layout, settings.interface, null)?
  store.save(root, {...base, owned: outcome.owned, reconcile: false})
  Ok("released and withdrew the owned configuration")
}
