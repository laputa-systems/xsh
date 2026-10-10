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
  match linux.run_in_namespaces(plan, unshare: ["user", "net"], map_root_user: true) {
    Ok(status) => {
      guard status.exited_with(0) else { return Ok(null) }
      Ok(out.read_text()?)
    }
    Err(_) => Ok(null)
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
