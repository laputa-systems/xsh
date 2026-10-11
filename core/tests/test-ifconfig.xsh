use core.lib.nettools as nettools

# The expected text below was produced by the reference tool (net-tools 2.x)
# for the same interface state; the counters are synthetic so that the layout
# of every column is pinned without depending on kernel traffic.

pure ether(name: Str, flags: Int) -> nettools.Interface {
  {
    name: name, index: 2, flags: flags, mtu: 1500, hwtype: 1, hwaddr: b"\x02\x00\x00\x00\x00\x01", txqueuelen: 1000,
    counters: {
      rx_packets: 2, rx_bytes: 1000, rx_errors: 0, rx_dropped: 0, rx_overruns: 0, rx_frame: 0,
      tx_packets: 1, tx_bytes: 70, tx_errors: 0, tx_dropped: 0, tx_overruns: 0, tx_carrier: 0, tx_collisions: 0,
    },
    ipv4: null, ipv6: [],
  }
}

test test_ifconfig_long_listing_matches_the_reference_layout {
  let plain = {...ether("d0", 195), ipv4: {local: "192.0.2.1", peer: "192.0.2.1", prefix: 24, broadcast: "192.0.2.255"}, ipv6: [
    {address: "2001:db8::5", prefix: 64, scope: 0},
    {address: "fe80::ff:fe00:1", prefix: 64, scope: 253},
  ]}
  assert nettools.long_listing(plain) == """d0: flags=195<UP,BROADCAST,RUNNING,NOARP>  mtu 1500
        inet 192.0.2.1  netmask 255.255.255.0  broadcast 192.0.2.255
        inet6 2001:db8::5  prefixlen 64  scopeid 0x0<global>
        inet6 fe80::ff:fe00:1  prefixlen 64  scopeid 0x20<link>
        ether 02:00:00:00:00:01  txqueuelen 1000  (Ethernet)
        RX packets 2  bytes 1000 (1000.0 B)
        RX errors 0  dropped 0  overruns 0  frame 0
        TX packets 1  bytes 70 (70.0 B)
        TX errors 0  dropped 0 overruns 0  carrier 0  collisions 0

"""
}

test test_ifconfig_long_listing_loopback_alias_and_unspec_hardware {
  let loopback = {
    name: "lo", index: 1, flags: 73, mtu: 65536, hwtype: 772, hwaddr: b"\x00\x00\x00\x00\x00\x00", txqueuelen: 1000,
    counters: null, ipv4: {local: "127.0.0.1", peer: "127.0.0.1", prefix: 8, broadcast: null},
    ipv6: [{address: "::1", prefix: 128, scope: 254}],
  }
  assert nettools.long_listing(loopback) == """lo: flags=73<UP,LOOPBACK,RUNNING>  mtu 65536
        inet 127.0.0.1  netmask 255.0.0.0
        inet6 ::1  prefixlen 128  scopeid 0x10<host>
        loop  txqueuelen 1000  (Local Loopback)

"""
  # An alias shows the alias address and no statistics.
  let alias = {...ether("d0:1", 195), counters: null, ipv4: {local: "198.51.100.7", peer: "198.51.100.7", prefix: 25, broadcast: "198.51.100.127"}}
  assert nettools.long_listing(alias) == """d0:1: flags=195<UP,BROADCAST,RUNNING,NOARP>  mtu 1500
        inet 198.51.100.7  netmask 255.255.255.128  broadcast 198.51.100.127
        ether 02:00:00:00:00:01  txqueuelen 1000  (Ethernet)

"""
  # A hardware type without a legacy name prints as unspec, 16 dash-separated bytes.
  let tunnel = {...ether("gre1", 144), hwtype: 778, hwaddr: b"\x0a\x00\x00\x02", counters: null}
  assert nettools.long_listing(tunnel) == """gre1: flags=144<POINTOPOINT,NOARP>  mtu 1500
        unspec 0A-00-00-02-00-00-00-00-00-00-00-00-00-00-00-00  txqueuelen 1000  (UNSPEC)

"""
  let ipip = {...ether("ipip1", 144), hwtype: 768, hwaddr: b"", counters: null}
  assert nettools.long_listing(ipip) == """ipip1: flags=144<POINTOPOINT,NOARP>  mtu 1500
        tunnel   txqueuelen 1000  (IPIP Tunnel)

"""
}

test test_ifconfig_flags_print_as_a_signed_16_bit_number_in_the_reference_order {
  # DYNAMIC is bit 15, which the reference prints as a negative number.
  let dynamic = nettools.long_listing(ether("d0", 4096 + 2 + 32768 + 2048 + 256 + 128 + 64 + 1))
  assert dynamic.starts_with("d0: flags=-26173<UP,BROADCAST,RUNNING,NOARP,PROMISC,SLAVE,MULTICAST,DYNAMIC>  mtu 1500\n")
  let bond = nettools.long_listing(ether("bond1", 7171))
  assert bond.starts_with("bond1: flags=7171<UP,BROADCAST,SLAVE,MASTER,MULTICAST>  mtu 1500\n")
  assert nettools.long_listing(ether("x", 0)).starts_with("x: flags=0<>  mtu 1500\n")
}

test test_ifconfig_short_listing_rows_and_flag_letters {
  assert nettools.SHORT_HEADER == "Iface      MTU    RX-OK RX-ERR RX-DRP RX-OVR    TX-OK TX-ERR TX-DRP TX-OVR Flg"
  assert nettools.short_row(ether("d0", 195)) == "d0               1500        2      0      0 0             1      0      0      0 BORU"
  let wide = {...ether("verylongname012", 4163), mtu: 65536}
  assert nettools.short_row(wide) == "verylongname012 65536        2      0      0 0             1      0      0      0 BMRU"
  let alias = {...ether("d0:1", 195), counters: null}
  assert nettools.short_row(alias) == "d0:1             1500      - no statistics available -                        BORU"
  # Letters in the reference order: promiscuous before NOARP, point-to-point after.
  assert nettools.flag_letters(512 + 2 + 4096 + 256 + 32 + 128 + 64 + 1) == "ABMPNORU"
  assert nettools.flag_letters(2 + 128 + 2048 + 64 + 1) == "BOsRU"
  assert nettools.flag_letters(2 + 4096 + 1024 + 2048 + 1) == "BMsmU"
  assert nettools.flag_letters(128 + 16) == "OP"
  assert nettools.flag_letters(2 + 32768 + 256 + 128 + 2048 + 64 + 1) == "BdPOsRU"
  assert nettools.flag_letters(0) == "[NO FLAGS]"
}

test test_ifconfig_byte_counts_step_by_1024_with_a_strict_comparison {
  assert nettools.human_bytes(0) == "0.0 B"
  assert nettools.human_bytes(1000) == "1000.0 B"
  assert nettools.human_bytes(1024) == "1024.0 B"
  assert nettools.human_bytes(1056) == "1.0 KiB"
  assert nettools.human_bytes(105880) == "103.3 KiB"
  assert nettools.human_bytes(1112) == "1.0 KiB"
  assert nettools.human_bytes(1504) == "1.4 KiB"
  assert nettools.human_bytes(1048576) == "1024.0 KiB"
  assert nettools.human_bytes(1048636) == "1.0 MiB"
  assert nettools.human_bytes(1073741825) == "1.0 GiB"
  assert nettools.human_bytes(1099511627777) == "1.0 TiB"
}

test test_ifconfig_interfaces_sort_in_the_reference_order {
  let names = ["verylongname012", "eth10", "eth2a", "eth2", "tunl0", "10a", "Eth3", "abc", "eth1", "br-x", "d_1", "lo", "d0:1", "d0", "d1"]
  let items = [ether(name, 1) for name in names]
  let sorted = [item.name for item in nettools.sort_interfaces(items)]
  assert sorted == ["10a", "Eth3", "abc", "br-x", "d0", "d0:1", "d1", "d_1", "eth1", "eth2", "eth10", "eth2a", "lo", "tunl0", "verylongname012"]
  # Numbers compare numerically only when both remainders are plain numbers.
  assert nettools.name_order("a2", "a10") < 0
  assert nettools.name_order("a10b", "a2b") < 0
  assert nettools.name_order("c001", "c01") < 0
  assert nettools.name_order("c1", "c010") < 0
  assert nettools.name_order("e1", "e10") < 0
  assert nettools.name_order("e10", "e1a") < 0
  assert nettools.name_order("same", "same") == 0
}

# A private network namespace with a dummy interface, entered through a child
# program that sets the fixture up and then runs the applet once per command.
proc session(ctx: TestContext, setup: List[Str], commands: List[List[Str]]) [fs, process, error] -> Result[Str] {
  let root = test.temp_dir(ctx, name: "ifconfig-ns")?
  let source = f"""use lib.nettools_fixture as fixture

proc main(...argv: List[Str]) [fs, process, net, io, error] {{
{setup.join("\n")}
  fixture.run_commands(argv)
}}
"""
  let child = test.temp_file(ctx, name: "session.xsh", contents: bytes.from_text(source))?
  var argv = [ctx.xsh_bin.display(), child.display(), ctx.xsh_bin.display(), fp"{ctx.core_dir}/ifconfig.xsh".display()]
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

const ADDRESSED = [
  "fixture.link_add_dummy(\"d0\", \"02:00:00:00:00:01\")",
  "fixture.link_up(\"d0\")",
  "fixture.address_add4(\"d0\", \"192.0.2.1\", 24, \"192.0.2.255\", \"\")",
  "fixture.address_add4(\"d0\", \"198.51.100.7\", 25, \"198.51.100.127\", \"d0:1\")",
  "fixture.address_add6(\"d0\", \"2001:db8::5\", 64)",
  "fixture.link_add_dummy(\"d1\", \"02:00:00:00:00:02\")",
]

# The counter lines depend on kernel traffic, so only the stable lines of a
# real listing are compared.
pure stable_lines(text: Str) -> List[Str] {
  [line for line in text.lines() if line.find("packets") == null and line.find("errors") == null]
}

test test_ifconfig_lists_interfaces_addresses_and_aliases_from_the_kernel { |ctx|
  let transcript = session(ctx, ADDRESSED, [["d0"], ["-s", "d0"], ["d0:1"], ["d1"], ["-a", "-s"], ["nosuch0"]])?
  if transcript == "" { return }
  let blocks = transcript.split("$ ")
  let long = stable_lines(blocks[1])
  assert long[0] == "d0"
  assert long[1] == "d0: flags=195<UP,BROADCAST,RUNNING,NOARP>  mtu 1500"
  assert long[2] == "        inet 192.0.2.1  netmask 255.255.255.0  broadcast 192.0.2.255"
  assert long[3] == "        inet6 2001:db8::5  prefixlen 64  scopeid 0x0<global>"
  assert long[4] == "        inet6 fe80::ff:fe00:1  prefixlen 64  scopeid 0x20<link>"
  assert long[5] == "        ether 02:00:00:00:00:01  txqueuelen 1000  (Ethernet)"
  assert blocks[2].find("d0               1500") != null
  assert blocks[2].find("BORU") != null
  assert blocks[3].find("d0:1: flags=195<UP,BROADCAST,RUNNING,NOARP>  mtu 1500") != null
  assert blocks[3].find("        inet 198.51.100.7  netmask 255.255.255.128  broadcast 198.51.100.127") != null
  assert blocks[3].find("RX packets") == null
  # A down interface is listed by name and hidden from the default listing.
  assert blocks[4].find("d1: flags=130<BROADCAST,NOARP>  mtu 1500") != null
  assert blocks[5].find("d0:1 ") != null and blocks[5].find("no statistics available") != null
  assert blocks[5].find("d1               1500") != null
  assert blocks[6].find("nosuch0: error fetching interface information: Device not found") != null
  assert blocks[6].find("rc=1") != null
}

test test_ifconfig_default_listing_hides_down_interfaces { |ctx|
  let transcript = session(ctx, ADDRESSED, [[]])?
  if transcript == "" { return }
  assert transcript.find("d0: flags=195") != null
  assert transcript.find("d1: flags") == null
}

test test_ifconfig_sets_flags_mtu_addresses_and_hardware_address { |ctx|
  let setup = [
    "fixture.link_add_dummy(\"d0\", \"02:00:00:00:00:01\")",
  ]
  let transcript = session(
    ctx,
    setup,
    [
      ["d0", "192.0.2.1", "netmask", "255.255.255.0", "up"],
      ["d0", "broadcast", "192.0.2.200", "mtu", "1400"],
      ["d0", "hw", "ether", "02:00:00:00:00:99"],
      ["d0", "promisc", "allmulti", "-arp"],
      ["d0"],
      ["d0", "-promisc", "-allmulti", "arp", "dynamic"],
      ["-s", "d0"],
      ["d0", "-dynamic", "trailers", "multicast"],
      ["d0", "down"],
      ["d0"],
    ],
  )?
  if transcript == "" { return }
  let blocks = transcript.split("$ ")
  assert blocks[1] == "d0 192.0.2.1 netmask 255.255.255.0 up\nrc=0\n"
  assert blocks[2] == "d0 broadcast 192.0.2.200 mtu 1400\nrc=0\n"
  assert blocks[3] == "d0 hw ether 02:00:00:00:00:99\nrc=0\n"
  let lines = stable_lines(blocks[5])
  assert lines[1] == "d0: flags=963<UP,BROADCAST,RUNNING,NOARP,PROMISC,ALLMULTI>  mtu 1400"
  assert lines[2] == "        inet 192.0.2.1  netmask 255.255.255.0  broadcast 192.0.2.200"
  assert blocks[5].find("        ether 02:00:00:00:00:99  txqueuelen 1000  (Ethernet)") != null
  assert blocks[7].find("BdRU") != null
  assert blocks[10].find("d0: flags=4098<BROADCAST,MULTICAST>  mtu 1400") != null
}

test test_ifconfig_adds_and_deletes_ipv6_addresses_and_renames { |ctx|
  let setup = [
    "fixture.link_add_dummy(\"d0\", \"02:00:00:00:00:01\")",
    "fixture.link_up(\"d0\")",
  ]
  let transcript = session(
    ctx,
    setup,
    [
      ["d0", "add", "2001:db8::8/64"],
      ["d0", "inet6", "add", "2001:db8::9"],
      ["d0"],
      ["d0", "del", "2001:db8::8/64"],
      ["d0", "del", "2001:db8::9"],
      ["d0", "del", "2001:db8::9"],
      ["d0", "add", "2001:db8::1/200"],
      ["d0", "add", "notanaddr"],
      ["d0", "down"],
      ["d0", "name", "e0"],
      ["-a", "-s"],
    ],
  )?
  if transcript == "" { return }
  let blocks = transcript.split("$ ")
  assert blocks[1] == "d0 add 2001:db8::8/64\nrc=0\n"
  assert blocks[2] == "d0 inet6 add 2001:db8::9\nrc=0\n"
  assert blocks[3].find("inet6 2001:db8::8  prefixlen 64  scopeid 0x0<global>") != null
  assert blocks[3].find("inet6 2001:db8::9  prefixlen 128  scopeid 0x0<global>") != null
  assert blocks[4] == "d0 del 2001:db8::8/64\nrc=0\n"
  assert blocks[5] == "d0 del 2001:db8::9\nrc=0\n"
  assert blocks[6] == "d0 del 2001:db8::9\n2> SIOCDIFADDR: Address not available\nrc=1\n"
  assert blocks[7].find("rc=3") != null and blocks[7].find("Usage:") != null
  assert blocks[8].find("2> notanaddr: Unknown host") != null and blocks[8].find("rc=1") != null
  assert blocks[10] == "d0 name e0\nrc=0\n"
  assert blocks[11].find("e0 ") != null and blocks[11].find("d0 ") == null
}

test test_ifconfig_reports_errors_in_the_reference_wording { |ctx|
  let setup = ["fixture.link_add_dummy(\"d0\", \"02:00:00:00:00:01\")"]
  let transcript = session(
    ctx,
    setup,
    [
      ["nosuch0", "down"],
      ["-v", "nosuch0", "up"],
      ["-z"],
      ["d0", "mtu"],
      ["d0", "netmask", "255.255.0.256"],
      ["d0", "nosuchhost.invalid"],
      ["d0", "hw", "ether", "02:00:00:00:00:zz"],
      ["d0", "hw", "ax25", "02:00:00:00:00:77"],
      ["d0", "mtu", "abc"],
      ["d0", "netmask", "255.255.255.0"],
      ["d0", "-broadcast"],
      ["d0", "irq", "3"],
      ["d0", "media", "10baseT"],
      ["d0", "tunnel", "1.2.3.4"],
      ["d0", "unix"],
      ["--version"],
    ],
  )?
  if transcript == "" { return }
  let blocks = transcript.split("$ ")
  assert blocks[1] == "nosuch0 down\n2> nosuch0: ERROR while getting interface flags: No such device\nrc=255\n"
  assert blocks[2] == "-v nosuch0 up\n2> nosuch0: ERROR while getting interface flags: No such device\n2> WARNING: at least one error occured. (-1)\nrc=255\n"
  assert blocks[3] == "-z\n2> ifconfig: option `-z' not recognised.\n2> ifconfig: `--help' gives usage information.\nrc=1\n"
  assert blocks[4].find("rc=3") != null and blocks[4].find("2> Usage:") != null
  assert blocks[5].find("2> 255.255.0.256: Unknown host\n2> ifconfig: `--help' gives usage information.\nrc=1\n") != null
  assert blocks[6].find("2> nosuchhost.invalid: Unknown host") != null
  assert blocks[7] == "d0 hw ether 02:00:00:00:00:zz\n2> 02:00:00:00:00:zz: invalid ether address.\nrc=1\n"
  assert blocks[8].find("rc=1") != null and blocks[8].find("ax25") != null
  assert blocks[9].find("2> ifconfig: invalid mtu 'abc'") != null
  assert blocks[10] == "d0 netmask 255.255.255.0\n2> SIOCSIFNETMASK: Address not available\nrc=1\n"
  assert blocks[11] == "d0 -broadcast\n2> Warning: Interface d0 still in BROADCAST mode.\nrc=0\n"
  # Words that need a device-private or map ioctl are refused by name.
  assert blocks[12] == "d0 irq 3\n2> ifconfig: irq is not supported\nrc=1\n"
  assert blocks[13] == "d0 media 10baseT\n2> ifconfig: media is not supported\nrc=1\n"
  assert blocks[14] == "d0 tunnel 1.2.3.4\n2> ifconfig: tunnel is not supported\nrc=1\n"
  assert blocks[15].find("unix: Unknown host") != null
  assert blocks[16].find("ifconfig (XSH core)") != null
}
