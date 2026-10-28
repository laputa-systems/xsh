#!/bin/xsh
use lib.dhcp_client as client
use lib.dhcp_live as live
use lib.dhcp_wire as wire

pure notify_descriptor(raw: Result[Str, Error]) -> Int? {
  if let Ok(text) = raw {
    if let Ok(descriptor) = text.parse_int() {
      return descriptor
    }
  }
  null
}

## Supervised DHCPv4 client for one interface. It runs in the foreground, takes
## the interface lock, reports readiness on NOTIFY_FD once the interface is
## resolved, the lock is held and its sockets are open, and runs until it is
## stopped. With --once it acquires one lease, configures it and exits; the
## lease is then not renewed and expires unmanaged.
cli main(
  interface: Str,
  lease_dir = p"/var/lib/dhcpc",
  run_dir = p"/run/dhcpc",
  resolv_dir = p"/etc",
  resolv_name = "resolv.conf",
  static_file = p"/etc/dhcpc/resolv.static",
  metric: UInt = 100,
  once = false,
  seconds: UInt = 10,
) [fs, process, net, env, time, error, io] {
  let settings = {...live.default_settings(interface), lease_dir: lease_dir, run_dir: run_dir, resolv_dir: resolv_dir, resolv_name: resolv_name, static_path: static_file, metric: metric}
  if once {
    let session = live.supervise(settings, .OneShot(seconds * 1000), null)?
    guard let lease = session.machine.lease else { return }
    print f"address={wire.ip_text(lease.address)}/{lease.prefix} server={wire.ip_text(lease.server)} lease={lease.lease_secs}s"
  } else {
    let notify = notify_descriptor(e"NOTIFY_FD")
    let _ = live.supervise(settings, .Supervised, notify)?
  }
}
