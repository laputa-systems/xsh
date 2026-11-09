# Runs `ip` under the `linux` test fake, whose fixed interfaces and routes keep
# the output independent of the host.
proc ip_output(ctx: TestContext, argv: List[Str]) [fs, process, error] -> Str {
  test.linux_fake(ctx)
  let source = fp"{ctx.core_dir}/ip.xsh".read_text()?
  let output = test.expect(ctx, source, status: 0, args: argv, env: {}, stdin: b"", name: "ip")?
  output.stdout
}

test test_ip_addr_smoke { |ctx|
  let output = ip_output(ctx, ["addr"])
  assert "eth0" in output
}

test test_ip_route_list_patterns_accept_exact_command_forms { |ctx|
  let route = ip_output(ctx, ["route"])
  let explicit = ip_output(ctx, ["route", "show"])
  assert route == explicit
}

test test_ip_addr_alternatives_accept_address_show_and_device_forms { |ctx|
  let short = ip_output(ctx, ["addr"])
  for argv in [["address"], ["addr", "show"], ["address", "show"]] {
    assert ip_output(ctx, argv) == short
  }

  let device = ip_output(ctx, ["addr", "dev", "eth0"])
  for argv in [["address", "dev", "eth0"], ["addr", "show", "dev", "eth0"], ["address", "show", "dev", "eth0"]] {
    assert ip_output(ctx, argv) == device
  }
}

# The real `ip`, run in a new network namespace of its own, lists that
# namespace and not the host's devices: sysfs is a mount-scoped view of the
# namespace that mounted it, so `ip` must read interfaces and routes over
# netlink.
proc applet_in_new_netns(ctx: TestContext, tool: Str, args: List[Str]) [fs, process, error] -> Result[Str?] {
  let root = test.temp_dir(ctx, name: "ip-netns")?
  let out = fp"{root}/out"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/{tool}.xsh".display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, b"", out)
  if let Ok(status) = linux.run_in_namespaces(plan, unshare: ["user", "net"], map_root_user: true) {
    guard status.exited_with(0) else { return Ok(null) }
    Ok(out.read_text()?)
  } else {
    Ok(null)
  }
}

test test_ip_in_a_new_network_namespace_lists_only_that_namespace { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("network namespaces are Linux-only")
    return
  }
  let addr = applet_in_new_netns(ctx, "ip", ["addr"])?
  guard let listing = addr else {
    test.skip("the kernel refuses unprivileged user and network namespaces")
    return
  }
  # A new namespace holds a down loopback device and, where tunnel modules
  # are loaded, their fallback devices, which the outer namespace has too.
  let fallback = ["tunl0", "gre0", "gretap0", "erspan0", "ip_vti0", "ip6_vti0", "sit0", "ip6tnl0", "ip6gre0"]
  assert "lo:" in listing, listing
  for iface in linux.interfaces()? {
    continue when iface.name == "lo" or iface.name in fallback
    assert !(f"{iface.name}:" in listing), f"{iface.name} leaked from the outer namespace into: {listing}"
  }

  let route = applet_in_new_netns(ctx, "ip", ["route"])?
  assert route == "", "a new network namespace has no routes"
}

test test_ifconfig_route_and_arp_list_only_the_new_network_namespace { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("network namespaces are Linux-only")
    return
  }
  let interfaces = applet_in_new_netns(ctx, "ifconfig", ["-a"])?
  guard let listing = interfaces else {
    test.skip("the kernel refuses unprivileged user and network namespaces")
    return
  }
  let fallback = ["tunl0", "gre0", "gretap0", "erspan0", "ip_vti0", "ip6_vti0", "sit0", "ip6tnl0", "ip6gre0"]
  assert "lo: flags=" in listing, listing
  for iface in linux.interfaces()? {
    continue when iface.name == "lo" or iface.name in fallback
    assert !(f"{iface.name}: " in listing), f"{iface.name} leaked from the outer namespace into: {listing}"
  }

  let routes = applet_in_new_netns(ctx, "route", ["-n"])?
  assert routes == "Kernel IP routing table\nDestination     Gateway         Genmask         Flags Metric Ref    Use Iface\n", "a new network namespace has no IPv4 routes"
  let neighbours = applet_in_new_netns(ctx, "arp", ["-an"])?
  assert neighbours == "", "a new network namespace has no ARP entries"
}

# `ip` changes links, addresses and routes through the same typed operations
# a controller uses, and its listing is rendered from the observation those
# operations converge to.
test test_ip_changes_links_addresses_and_routes_and_lists_what_the_kernel_converged_to { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("network namespaces are Linux-only")
    return
  }
  let root = test.temp_dir(ctx, name: "ip-change")?
  let source = """use lib.nettools_fixture as fixture

proc main(...argv: List[Str]) [fs, process, net, io, error] {
  fixture.link_add_veth("v0", "v1", "02:00:00:00:00:02", "02:00:00:00:00:03")
  fixture.run_commands(argv)
}
"""
  let child = test.temp_file(ctx, name: "session.xsh", contents: bytes.from_text(source))?
  let commands = [
    ["link", "set", "dev", "v0", "up"],
    ["link", "set", "dev", "v1", "up"],
    ["addr", "add", "192.0.2.2/24", "broadcast", "192.0.2.255", "dev", "v0"],
    ["addr", "add", "192.0.2.2/24", "dev", "v0"],
    ["addr", "add", "2001:db8::2/64", "dev", "v0"],
    ["route", "add", "default", "via", "192.0.2.1", "dev", "v0", "metric", "100"],
    ["route", "add", "203.0.113.0/24", "dev", "v0"],
    ["route", "add", "2001:db8:5::/48", "via", "2001:db8::1", "dev", "v0", "metric", "1024"],
    ["addr", "show", "dev", "v0"],
    ["route"],
    ["route", "del", "default"],
    ["route", "del", "default"],
    ["addr", "del", "192.0.2.2/24", "dev", "v0"],
    ["link", "set", "dev", "v0", "down"],
    ["link", "show", "dev", "v0"],
    ["route"],
    ["addr", "add", "192.0.2.2/24", "dev", "nosuch0"],
  ]
  var argv = [ctx.xsh_bin.display(), child.display(), ctx.xsh_bin.display(), fp"{ctx.core_dir}/ip.xsh".display()]
  for command in commands {
    argv = argv.extend(command).push("--")
  }
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let child_env = {LC_ALL: "C", XSH_EXECUTION_PHRASE: "", XSH_MODULE_PATH: ctx.core_dir.display()}
  let plan = process.command_argv(ctx.xsh_bin, argv, root, child_env, b"", out, err, timeout: 60s)
  guard let status = linux.run_in_namespaces(plan, unshare: ["user", "net"], map_root_user: true) else {
    test.skip("the kernel refuses unprivileged user and network namespaces")
    return
  }
  assert status.shell_code()? == 0, err.read_text()?
  let transcript = out.read_text()?
  assert transcript == """$ link set dev v0 up
rc=0
$ link set dev v1 up
rc=0
$ addr add 192.0.2.2/24 broadcast 192.0.2.255 dev v0
rc=0
$ addr add 192.0.2.2/24 dev v0
2> RTNETLINK answers: File exists
rc=2
$ addr add 2001:db8::2/64 dev v0
rc=0
$ route add default via 192.0.2.1 dev v0 metric 100
rc=0
$ route add 203.0.113.0/24 dev v0
rc=0
$ route add 2001:db8:5::/48 via 2001:db8::1 dev v0 metric 1024
rc=0
$ addr show dev v0
v0: mtu 1500 flags UP,BROADCAST,RUNNING,MULTICAST
    link/ether 02:00:00:00:00:02
    inet 192.0.2.2/24
    inet6 2001:db8::2/64
    inet6 fe80::ff:fe00:2/64
rc=0
$ route
default via 192.0.2.1 dev v0 metric 100
192.0.2.0/24 dev v0 metric 0
203.0.113.0/24 dev v0 metric 0
2001:db8::/64 dev v0 metric 256
2001:db8:5::/48 via 2001:db8::1 dev v0 metric 1024
fe80::/64 dev v1 metric 256
fe80::/64 dev v0 metric 256
rc=0
$ route del default
rc=0
$ route del default
2> ip: no route matches "default"
rc=1
$ addr del 192.0.2.2/24 dev v0
rc=0
$ link set dev v0 down
rc=0
$ link show dev v0
v0: mtu 1500 flags BROADCAST,MULTICAST
    link/ether 02:00:00:00:00:02
rc=0
$ route
fe80::/64 dev v1 metric 256
rc=0
$ addr add 192.0.2.2/24 dev nosuch0
2> ip: cannot find device "nosuch0"
rc=1
""", transcript
}
