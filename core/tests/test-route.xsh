use core.lib.nettools as nettools

# Expected tables were produced by the reference tool for the same routes.

pure row(destination: Str, gateway: Str, mask: Str, flags: Str, metric: Int, iface: Str) -> nettools.RouteRow {
  {
    destination: destination, gateway: gateway, mask: mask, flags: flags, metric: metric, ref: 0, use: 0,
    iface: iface, mss: 0, window: 0, irtt: 0, reject: flags == "!", prefix: 24,
  }
}

test test_route_rows_follow_the_reference_columns {
  let gateway = row("0.0.0.0", "192.0.2.254", "0.0.0.0", "UG", 7, "d0")
  assert nettools.route4_row(gateway, gateway.destination, gateway.gateway, 0) == "0.0.0.0         192.0.2.254     0.0.0.0         UG    7      0        0 d0"
  assert nettools.route4_row(gateway, "default", "192.0.2.254", 0) == "default         192.0.2.254     0.0.0.0         UG    7      0        0 d0"
  let host = row("10.30.0.2", "192.0.2.2", "255.255.255.255", "UGH", 0, "d0")
  assert nettools.route4_row(host, host.destination, host.gateway, 0) == "10.30.0.2       192.0.2.2       255.255.255.255 UGH   0      0        0 d0"
  let tuned = {...row("10.40.0.0", "192.0.2.2", "255.255.255.0", "UG", 9, "d0"), mss: 1400, window: 5000, irtt: 50}
  assert nettools.route4_row(tuned, tuned.destination, tuned.gateway, 1) == "10.40.0.0       192.0.2.2       255.255.255.0   UG     1400 5000      50 d0"
  assert nettools.route4_row(tuned, tuned.destination, tuned.gateway, 2) == "10.40.0.0       192.0.2.2       255.255.255.0   UG    9      0        0 d0       1400  5000   50"
  # A reject route has no gateway, reference count, or interface to show.
  let reject = {...row("10.50.0.0", "0.0.0.0", "255.255.255.0", "!", 0, "*"), reject: true}
  assert nettools.route4_row(reject, reject.destination, reject.gateway, 0) == "10.50.0.0       -               255.255.255.0   !     0      -        0 -"
  assert nettools.route4_row(reject, reject.destination, reject.gateway, 1) == "10.50.0.0       -               255.255.255.0   !         - -          - -"
  assert nettools.route4_row(reject, reject.destination, reject.gateway, 2) == "10.50.0.0       -               255.255.255.0   !     0      -        0 -        -     -      -"
  assert nettools.ROUTE4_HEADER == "Kernel IP routing table\nDestination     Gateway         Genmask         Flags Metric Ref    Use Iface"
  assert nettools.ROUTE4_EXTENDED_HEADER == "Kernel IP routing table\nDestination     Gateway         Genmask         Flags   MSS Window  irtt Iface"
  assert nettools.ROUTE4_EXTRA_HEADER == "Kernel IP routing table\nDestination     Gateway         Genmask         Flags Metric Ref    Use Iface    MSS   Window irtt"
}

test test_route_ipv6_rows_and_text_helpers {
  assert nettools.ROUTE6_HEADER == "Kernel IPv6 routing table\nDestination                    Next Hop                   Flag Met Ref  Use If"
  let gateway = {destination: "2001:db8:1::", prefix: 48, nexthop: "fe80::1", flags: 3, metric: 1024, ref: 3, use: 0, iface: "d1"}
  assert nettools.route6_row(gateway, "2001:db8:1::/48", "fe80::1") == "2001:db8:1::/48                fe80::1                    UG   1024 3      0 d1"
  let local = {destination: "fe80::1", prefix: 128, nexthop: "::", flags: 2097153, metric: 0, ref: 2, use: 0, iface: "d1"}
  assert nettools.route6_row(local, "fe80::1/128", "::") == "fe80::1/128                    ::                         Un   0   2      0 d1"
  let reject = {destination: "::", prefix: 0, nexthop: "::", flags: 2097664, metric: -1, ref: 1, use: 0, iface: "lo"}
  assert nettools.route6_row(reject, "::/0", "::") == "::/0                           ::                         !n   -1  1      0 lo"
  # A destination longer than its column pushes the rest right, as the reference does.
  let long = {destination: "2001:db8:1234:5678:9abc:def0:1234:5678", prefix: 128, nexthop: "::", flags: 1, metric: 256, ref: 2, use: 0, iface: "d1"}
  assert nettools.route6_row(long, "2001:db8:1234:5678:9abc:def0:1234:5678/128", "::") == "2001:db8:1234:5678:9abc:def0:1234:5678/128 ::                         U    256 2      0 d1"

  assert nettools.ipv6_parse("2001:db8::5") == b"\x20\x01\x0d\xb8\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x05"
  assert nettools.ipv6_parse("::") == bytes.zero(16)?
  assert nettools.ipv6_parse("::1") == b"\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x01"
  assert nettools.ipv6_parse("::ffff:192.0.2.1") == b"\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\xff\xff\xc0\x00\x02\x01"
  assert nettools.ipv6_parse("1:2:3:4:5:6:7:8") != null
  assert nettools.ipv6_parse("1:2:3:4:5:6:7") == null
  assert nettools.ipv6_parse("1::2::3") == null
  assert nettools.ipv6_parse("12345::") == null
  assert nettools.ipv6_parse("notanaddress") == null
  assert nettools.compress_ipv6("2001:0db8:0000:0000:0000:0000:0000:0005") == "2001:db8::5"
  assert nettools.compress_ipv6("fe80:0000:0000:0000:0000:0000:0000:0000") == "fe80::"
  assert nettools.compress_ipv6("0000:0000:0000:0000:0000:0000:0000:0000") == "::"
  assert nettools.compress_ipv6("2001:0db8:0000:0001:0000:0000:0000:0001") == "2001:db8:0:1::1"
  assert nettools.compress_ipv6("2001:0db8:0000:0001:0001:0001:0001:0001") == "2001:db8:0:1:1:1:1:1"
}

test test_route_masks_and_addresses {
  assert nettools.prefix_mask(0) == b"\x00\x00\x00\x00"
  assert nettools.prefix_mask(12) == b"\xff\xf0\x00\x00"
  assert nettools.prefix_mask(32) == b"\xff\xff\xff\xff"
  assert nettools.mask_prefix(b"\xff\xf0\x00\x00") == 12
  assert nettools.mask_prefix(b"\xff\x00\xff\x00") == null
  assert nettools.netmask_parse("0xffffff00") == b"\xff\xff\xff\x00"
  assert nettools.netmask_parse("255.255.0") == null
  assert nettools.ipv4_parse("10.1.2.3") == b"\x0a\x01\x02\x03"
  assert nettools.ipv4_parse("10.1.2.256") == null
  assert nettools.ipv4_parse("10.1.2") == null
}

# A private network namespace with two dummy interfaces, entered through a
# child program that sets the fixture up and then runs route once per command.
proc session(ctx: TestContext, extra: List[Str], commands: List[List[Str]]) [fs, process, error] -> Result[Str] {
  let root = test.temp_dir(ctx, name: "route-ns")?
  let setup = [
    "fixture.link_add_dummy(\"d0\", \"02:00:00:00:00:01\")",
    "fixture.link_up(\"d0\")",
    "fixture.address_add4(\"d0\", \"192.0.2.1\", 24, \"192.0.2.255\", \"\")",
    "fixture.link_add_dummy(\"d1\", \"02:00:00:00:00:02\")",
    "fixture.link_up(\"d1\")",
    "fixture.address_add4(\"d1\", \"10.1.0.1\", 16, \"10.1.255.255\", \"\")",
  ].extend(extra)
  let source = f"""use lib.nettools_fixture as fixture

proc main(...argv: List[Str]) [fs, process, io, error] {{
{setup.join("\n")}
  fixture.run_commands(argv)
}}
"""
  let child = test.temp_file(ctx, name: "session.xsh", contents: bytes.from_text(source))?
  var argv = [ctx.xsh_bin.display(), child.display(), ctx.xsh_bin.display(), fp"{ctx.core_dir}/route.xsh".display()]
  for command in commands {
    argv = argv.extend(command).push("--")
  }
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let child_env = {LC_ALL: "C", XSH_EXECUTION_PHRASE: "", XSH_MODULE_PATH: ctx.core_dir.display()}
  let plan = process.command_argv(ctx.xsh_bin, argv, root, child_env, b"", out, err, timeout: 60s)
  match linux.run_in_namespaces(plan, unshare: ["user", "net"], map_root_user: true) {
    Ok(status) => {
      if status.shell_code()? != 0 {
        test.skip(f"cannot build the interface fixture in a private network namespace: {err.read_text()?.trim()}")
        return ""
      }
      Ok(out.read_text()?)
    }
    Err(failure) => {
      test.skip(f"the kernel refuses a private user and network namespace: {failure.message}")
      Ok("")
    }
  }
}

test test_route_adds_routes_and_lists_them_in_the_reference_layout { |ctx|
  let transcript = session(
    ctx,
    [],
    [
      ["add", "default", "gw", "192.0.2.254"],
      ["add", "-net", "10.20.0.0", "netmask", "255.255.0.0", "dev", "d1"],
      ["add", "-net", "10.21.0.0/16", "gw", "10.1.0.5"],
      ["add", "-host", "10.30.0.1", "dev", "d0"],
      ["add", "-host", "10.30.0.2", "gw", "192.0.2.2"],
      ["add", "-net", "10.40.0.0", "netmask", "255.255.255.0", "gw", "192.0.2.2", "metric", "9", "mss", "1400", "window", "5000", "irtt", "50", "dev", "d0"],
      ["add", "-net", "10.50.0.0", "netmask", "255.255.255.0", "reject"],
      ["add", "10.61.0.5", "dev", "d1"],
      ["add", "default", "dev", "d1"],
      ["-n"],
      [],
      ["-ne"],
      ["-nee"],
    ],
  )?
  if transcript == "" { return }
  let blocks = transcript.split("$ ")
  for index in range(1, 10) {
    assert blocks[index].ends_with("rc=0\n"), blocks[index]
  }
  let table = """Kernel IP routing table
Destination     Gateway         Genmask         Flags Metric Ref    Use Iface
0.0.0.0         0.0.0.0         0.0.0.0         U     0      0        0 d1
0.0.0.0         192.0.2.254     0.0.0.0         UG    0      0        0 d0
10.1.0.0        0.0.0.0         255.255.0.0     U     0      0        0 d1
10.20.0.0       0.0.0.0         255.255.0.0     U     0      0        0 d1
10.21.0.0       10.1.0.5        255.255.0.0     UG    0      0        0 d1
10.30.0.1       0.0.0.0         255.255.255.255 UH    0      0        0 d0
10.30.0.2       192.0.2.2       255.255.255.255 UGH   0      0        0 d0
10.40.0.0       192.0.2.2       255.255.255.0   UG    9      0        0 d0
10.50.0.0       -               255.255.255.0   !     0      -        0 -
10.61.0.5       0.0.0.0         255.255.255.255 UH    0      0        0 d1
192.0.2.0       0.0.0.0         255.255.255.0   U     0      0        0 d0
"""
  assert blocks[10] == f"-n\n{table}rc=0\n", blocks[10]
  # Without -n the default route and unnamed addresses print as the reference does.
  assert blocks[11].find("default         0.0.0.0         0.0.0.0         U     0      0        0 d1\n") != null
  assert blocks[11].find("default         192.0.2.254     0.0.0.0         UG    0      0        0 d0\n") != null
  assert blocks[12].find("Flags   MSS Window  irtt Iface\n") != null
  assert blocks[12].find("10.40.0.0       192.0.2.2       255.255.255.0   UG     1400 5000      50 d0\n") != null
  assert blocks[13].find("Iface    MSS   Window irtt\n") != null
  assert blocks[13].find("10.40.0.0       192.0.2.2       255.255.255.0   UG    9      0        0 d0       1400  5000   50\n") != null
  assert blocks[13].find("10.50.0.0       -               255.255.255.0   !     0      -        0 -        -     -      -\n") != null
}

test test_route_deletes_by_destination_and_wildcard_metric { |ctx|
  let transcript = session(
    ctx,
    [],
    [
      ["add", "-net", "10.40.0.0", "netmask", "255.255.255.0", "gw", "192.0.2.2", "metric", "9", "dev", "d0"],
      ["add", "default", "gw", "192.0.2.254"],
      ["del", "-net", "10.40.0.0", "netmask", "255.255.255.0"],
      ["del", "-net", "10.40.0.0", "netmask", "255.255.255.0"],
      ["del", "default"],
      ["del", "-net", "10.99.0.0", "netmask", "255.255.0.0"],
      ["-n"],
    ],
  )?
  if transcript == "" { return }
  let blocks = transcript.split("$ ")
  assert blocks[3] == "del -net 10.40.0.0 netmask 255.255.255.0\nrc=0\n"
  assert blocks[4] == "del -net 10.40.0.0 netmask 255.255.255.0\n2> SIOCDELRT: No such process\nrc=7\n"
  assert blocks[5] == "del default\nrc=0\n"
  assert blocks[6] == "del -net 10.99.0.0 netmask 255.255.0.0\n2> SIOCDELRT: No such process\nrc=7\n"
  assert blocks[7] == "-n\nKernel IP routing table\nDestination     Gateway         Genmask         Flags Metric Ref    Use Iface\n10.1.0.0        0.0.0.0         255.255.0.0     U     0      0        0 d1\n192.0.2.0       0.0.0.0         255.255.255.0   U     0      0        0 d0\nrc=0\n"
}

test test_route_lists_reject_blackhole_and_throw_routes { |ctx|
  let extra = [
    "fixture.route_add_type4(\"100.64.0.0\", 10, 6)",
    "fixture.route_add_type4(\"100.100.0.0\", 16, 7)",
    "fixture.route_add_type4(\"100.101.0.0\", 16, 8)",
    "fixture.route_add_type4(\"100.102.0.0\", 16, 9)",
  ]
  let transcript = session(ctx, extra, [["-n"]])?
  if transcript == "" { return }
  assert transcript.find("100.64.0.0      0.0.0.0         255.192.0.0     U     0      0        0 *\n") != null
  assert transcript.find("100.100.0.0     -               255.255.0.0     !     0      -        0 -\n") != null
  assert transcript.find("100.101.0.0     -               255.255.0.0     !     0      -        0 -\n") != null
  assert transcript.find("100.102.0.0     0.0.0.0         255.255.0.0     U     0      0        0 *\n") != null
}

test test_route_adds_and_deletes_ipv6_routes { |ctx|
  let transcript = session(
    ctx,
    [],
    [
      ["-A", "inet6", "add", "2001:db8:9::/48", "dev", "d0", "metric", "7"],
      ["-A", "inet6", "add", "2001:db8:9::/48", "dev", "d0", "metric", "7"],
      ["-A", "inet6", "add", "2001:db8:c::1/128", "dev", "d0"],
      ["-A", "inet6", "add", "default", "dev", "d0", "metric", "9"],
      ["-A", "inet6", "-n"],
      ["-A", "inet6", "del", "2001:db8:9::/48", "dev", "d0"],
      ["-A", "inet6", "del", "2001:db8:9::/48", "metric", "7"],
      ["-A", "inet6", "add", "2001:db8:5::/200", "dev", "d0"],
      ["-A", "inet6", "add", "2001:db8:6::/48", "dev", "nosuch0"],
      ["-6", "-n"],
    ],
  )?
  if transcript == "" { return }
  let blocks = transcript.split("$ ")
  assert blocks[1] == "-A inet6 add 2001:db8:9::/48 dev d0 metric 7\nrc=0\n"
  assert blocks[2] == "-A inet6 add 2001:db8:9::/48 dev d0 metric 7\n2> SIOCADDRT: File exists\nrc=7\n"
  assert blocks[5].starts_with("-A inet6 -n\nKernel IPv6 routing table\nDestination                    Next Hop                   Flag Met Ref  Use If\n")
  assert blocks[5].find("2001:db8:9::/48                ::                         U    7   ") != null
  assert blocks[5].find("2001:db8:c::1/128              ::                         U    1   1      0 d0\n") != null
  assert blocks[5].find("::/0                           ::                         U    9   1      0 d0\n") != null
  # The default metric is 1, and a delete names the metric of the route it removes.
  assert blocks[6] == "-A inet6 del 2001:db8:9::/48 dev d0\n2> SIOCDELRT: No such process\nrc=7\n"
  assert blocks[7] == "-A inet6 del 2001:db8:9::/48 metric 7\nrc=0\n"
  assert blocks[8].find("rc=3") != null and blocks[8].find("inet6_route") != null
  assert blocks[9] == "-A inet6 add 2001:db8:6::/48 dev nosuch0\n2> SIOCADDRT: No such device\nrc=7\n"
  assert blocks[10].find("2001:db8:9::/48") == null
  assert blocks[10].find("2001:db8:c::1/128") != null
}

test test_route_errors_and_refused_forms { |ctx|
  let transcript = session(
    ctx,
    [],
    [
      ["add", "-net", "10.20.0.0", "netmask", "255.255.0.0", "dev", "d1"],
      ["add", "-net", "10.20.0.0", "netmask", "255.255.0.0", "dev", "d1"],
      ["add", "10.60.0.0", "netmask", "255.255.0.0", "dev", "d1"],
      ["add", "-net", "10.73.0.0", "netmask", "255.0.0.0", "dev", "d1"],
      ["add", "-net", "10.76.0.0", "netmask", "255.255.0.0"],
      ["add", "-net", "10.78.0.0", "d1"],
      ["add", "-net", "10.77.0.0", "netmask", "255.255.0.0", "dev", "nosuch0"],
      ["add", "-net", "16.0.0.0", "netmask", "255.0.0.0", "mss", "99999", "dev", "d1"],
      ["add", "-net", "16.0.0.0", "netmask", "255.0.0.0", "gw", "nosuchhost.invalid", "dev", "d1"],
      ["add", "-net", "16.0.0.0", "netmask", "255.0.0.0", "mod", "dev", "d1"],
      ["add", "-net", "16.0.0.0", "netmask", "255.0.0.0", "dyn", "dev", "d1"],
      ["add", "-net"],
      ["flush"],
      ["-A", "foo"],
      ["-z"],
      ["-V"],
    ],
  )?
  if transcript == "" { return }
  let blocks = transcript.split("$ ")
  assert blocks[2] == "add -net 10.20.0.0 netmask 255.255.0.0 dev d1\n2> SIOCADDRT: File exists\nrc=7\n"
  assert blocks[3].starts_with("add 10.60.0.0 netmask 255.255.0.0 dev d1\n2> route: netmask 0000ffff doesn't make sense with host route\n2> Usage: inet_route")
  assert blocks[3].ends_with("rc=3\n")
  assert blocks[4].starts_with("add -net 10.73.0.0 netmask 255.0.0.0 dev d1\n2> route: netmask doesn't match route address\n")
  assert blocks[5] == "add -net 10.76.0.0 netmask 255.255.0.0\n2> SIOCADDRT: No such device\nrc=7\n"
  assert blocks[6] == "add -net 10.78.0.0 d1\n2> SIOCADDRT: Invalid argument\nrc=7\n"
  assert blocks[7] == "add -net 10.77.0.0 netmask 255.255.0.0 dev nosuch0\n2> SIOCADDRT: No such device\nrc=7\n"
  assert blocks[8] == "add -net 16.0.0.0 netmask 255.0.0.0 mss 99999 dev d1\n2> route: Invalid MSS/MTU.\nrc=3\n"
  assert blocks[9].find("2> nosuchhost.invalid: Unknown host\n") != null and blocks[9].ends_with("rc=6\n")
  # mod, dyn, and reinstate are accepted by the reference but have no effect on Linux.
  assert blocks[10].starts_with("add -net 16.0.0.0 netmask 255.0.0.0 mod dev d1\n2> route: `mod' is accepted by net-tools but has no effect on Linux; not supported\n")
  assert blocks[10].ends_with("rc=3\n")
  assert blocks[11].find("`dyn'") != null and blocks[11].ends_with("rc=3\n")
  assert blocks[12].find("2> Usage: inet_route") != null and blocks[12].ends_with("rc=3\n")
  assert blocks[13].starts_with("flush\n2> Flushing `inet' routing table not supported\n")
  assert blocks[14] == "-A foo\n2> Unknown address family `foo'.\nrc=1\n"
  assert blocks[15].find("2> route: unrecognized option: z\n") != null and blocks[15].ends_with("rc=3\n")
  assert blocks[16].find("route (XSH core)") != null
}
