use core.lib.ethtool_decode as dec

# The device tests run the applet in a private network namespace of their own
# (a user and a network namespace made with unshare) holding a dummy device
# `d0`, a veth pair `v0`/`v1`, and `lo` brought up, so nothing touches the
# machine's interfaces. They skip where such a namespace cannot be made.

# Runs inside the namespace: creates the devices through rtnetlink, then runs
# each line of a command file as one ethtool invocation and leaves its output
# and status beside the file.
const SETUP = """
proc attribute(kind: Int, data: Bytes) [error] -> Result[Bytes] {
  let length = 4 + data.len()
  Ok(bytes.concat([bytes.pack_le(length, 2)?, bytes.pack_le(kind, 2)?, data, bytes.zero((4 - length % 4) % 4)?]))
}

proc cstring(text: Str) -> Bytes {
  bytes.concat([bytes.from_text(text), b"\\0"])
}

proc new_link(fd: Int, info: Bytes, name: Str) [process, error] -> Result[Unit] {
  let c = linux.net_constants()
  let flags = c.NLM_F_REQUEST.bit_or(c.NLM_F_ACK).bit_or(c.NLM_F_CREATE).bit_or(c.NLM_F_EXCL)
  let payload = bytes.concat([bytes.zero(16)?, attribute(3, cstring(name))?, attribute(18, info)?])
  let _ = linux.netlink_request(fd, c.RTM_NEWLINK, flags, payload)?
}

proc make_devices() [process, net, error] -> Result[Unit] {
  let c = linux.net_constants()
  let nl = linux.netlink_open(c.NETLINK_ROUTE)?
  new_link(nl, attribute(1, cstring("dummy"))?, "d0")?
  let peer = bytes.concat([bytes.zero(16)?, attribute(3, cstring("v1"))?])
  let data = attribute(2, attribute(1, peer)?)?
  new_link(nl, bytes.concat([attribute(1, cstring("veth"))?, data]), "v0")?

  let fd = linux.socket(c.AF_INET, c.SOCK_DGRAM)?
  let name = bytes.concat([bytes.from_text("lo"), bytes.zero(14)?])
  let _ = linux.ioctl(fd, c.SIOCSIFFLAGS, bytes.concat([name, bytes.pack_le(c.IFF_UP, 2)?]), 18)?
}

proc main(...argv: List[Str]) [fs, process, error] {
  if let Err(failure) = make_devices() {
    fp"{argv[2]}/unavailable".write(failure.message)?
    return
  }

  let xsh = argv[0]
  let script = argv[1]
  let out = fp"{argv[2]}"
  fp"{out}/ready".write("ready")?
  var index = 0
  for line in fp"{argv[3]}".read_text()?.lines() {
    let plan = process.command_argv(xsh, [xsh, script].extend(line.fields()), out, {LC_ALL: "C"}, b"", fp"{out}/{index}.out", fp"{out}/{index}.err", timeout: 20s)
    let status = process.run(plan)?
    fp"{out}/{index}.status".write(f"{status.shell_code()?}")?
    index += 1
  }
}
"""

type Ran = {status: Int, stdout: Str, stderr: Str}

# Runs each command in one fresh namespace and returns the results in order.
# Skips the calling test when the namespace or its devices cannot be made.
proc session(ctx: TestContext, commands: List[Str]) [fs, process, error] -> Result[List[Ran]] {
  let root = test.temp_dir(ctx, name: "ethtool")?
  let setup = test.temp_file(ctx, name: "setup.xsh", contents: bytes.from_text(SETUP))?
  let list = test.temp_file(ctx, name: "commands", contents: bytes.from_text(commands.join("\n") + "\n"))?
  let unshare = fp"{ctx.core_dir}/unshare.xsh"
  let ethtool = fp"{ctx.core_dir}/ethtool.xsh"
  let xsh = ctx.xsh_bin.display()
  let argv = [xsh, unshare.display(), "-r", "-n", xsh, setup.display(), xsh, ethtool.display(), root.display(), list.display()]
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""}, b"", fp"{root}/setup.out", fp"{root}/setup.err", timeout: 120s)
  let status = process.run(plan)?
  if ! fp"{root}/ready".exists() {
    let reason = if fp"{root}/unavailable".exists() { fp"{root}/unavailable".read_text()? } else { fp"{root}/setup.err".read_text()? }
    # A helper that does not compile is a bug in this test, not a host limit.
    assert "err[" not in reason, reason
    test.skip(f"cannot build a network namespace with dummy and veth devices: {reason.trim()}")
    return []
  }
  assert status.exited_with(0), fp"{root}/setup.err".read_text()?
  var results: List[Ran] = []
  for index in range(commands.len()) {
    results += [{
      status: fp"{root}/{index}.status".read_text()?.trim().parse_int()?,
      stdout: fp"{root}/{index}.out".read_text()?,
      stderr: fp"{root}/{index}.err".read_text()?,
    }]
  }
  Ok(results)
}

# The libc wording of EOPNOTSUPP differs by C library ("Operation not
# supported", "Not supported").
pure not_supported(line: Str) -> Bool {
  line.lower().ends_with("not supported")
}

pure position(lines: List[Str], text: Str) -> Int {
  for index in range(lines.len()) {
    return index when lines[index] == text
  }
  -1
}

test test_driver_information_matches_the_reference_layout { |ctx|
  let ran = session(ctx, ["-i d0", "-i v0", "-i lo", "-i nosuch0", "-i d0 foo", "-i"])?
  guard ! ran.is_empty() else { return }
  let dummy = ran[0].stdout.lines()
  assert ran[0].status == 0
  assert dummy[0] == "driver: dummy"
  assert dummy[2..] == [
    "firmware-version: ", "expansion-rom-version: ", "bus-info: ", "supports-statistics: no", "supports-test: no",
    "supports-eeprom-access: no", "supports-register-dump: no", "supports-priv-flags: no",
  ]
  assert ran[1].stdout.lines()[0] == "driver: veth"
  assert "supports-statistics: yes" in ran[1].stdout
  assert ran[2].status == 71
  assert ran[2].stderr.starts_with("Cannot get driver information: ")
  assert ran[3].status == 71
  assert ran[3].stderr == "Cannot get driver information: No such device\n"
  assert ran[4].status == 1
  assert ran[4].stderr == "ethtool: bad command line argument(s)\nFor more information run ethtool -h\n"
  assert ran[5].stderr == ran[4].stderr
}

test test_settings_report_matches_the_reference_layout { |ctx|
  let ran = session(ctx, ["v0", "d0", "lo", "nosuch0", "d0 -k"])?
  guard ! ran.is_empty() else { return }
  assert ran[0].status == 0
  assert ran[0].stdout.lines() == [
    "Settings for v0:",
    "\tSupported ports: [  ]",
    "\tSupported link modes:   Not reported",
    "\tSupported pause frame use: No",
    "\tSupports auto-negotiation: No",
    "\tSupported FEC modes: Not reported",
    "\tAdvertised link modes:  Not reported",
    "\tAdvertised pause frame use: No",
    "\tAdvertised auto-negotiation: No",
    "\tAdvertised FEC modes: Not reported",
    "\tSpeed: 10000Mb/s",
    "\tDuplex: Full",
    "\tAuto-negotiation: off",
    "\tPort: Twisted Pair",
    "\tPHYAD: 0",
    "\tTransceiver: internal",
    "\tMDI-X: Unknown",
    "\tLink detected: no",
  ], ran[0].stdout
  # A dummy device reports neither settings nor link state; loopback only
  # its link state.
  assert ran[1].status == 75 and ran[1].stdout == "No data available\n" and ran[1].stderr == ""
  assert ran[2].status == 0 and ran[2].stdout == "Settings for lo:\n\tLink detected: yes\n", ran[2].stdout
  assert ran[3].status == 75
  assert ran[3].stdout == "No data available\n"
  let pair = "netlink error: no device matches name (offset 24)\nnetlink error: No such device\n"
  assert ran[3].stderr == pair + pair + pair + pair + pair + pair + pair
  # Extra words after the device are refused, not ignored.
  assert ran[4].status == 1 and ran[4].stderr == "ethtool: unexpected parameter '-k'\n"
}

test test_features_report_groups_flags_and_marks_fixed_features { |ctx|
  let ran = session(ctx, ["-k d0", "-k v0", "-k lo", "-k nosuch0", "-k d0 extra"])?
  guard ! ran.is_empty() else { return }
  let lines = ran[0].stdout.lines()
  assert ran[0].status == 0
  assert lines[0] == "Features for d0:"
  assert lines[1..7] == [
    "rx-checksumming: off [fixed]",
    "tx-checksumming: on",
    "\ttx-checksum-ipv4: off [fixed]",
    "\ttx-checksum-ip-generic: on",
    "\ttx-checksum-ipv6: off [fixed]",
    "\ttx-checksum-fcoe-crc: off [fixed]",
  ]
  assert "scatter-gather: on" in lines and "\ttx-scatter-gather-fraglist: on" in lines
  assert "tcp-segmentation-offload: on" in lines
  assert "highdma: on" in lines and "loopback: off [fixed]" in lines
  # A feature no offload flag covers is listed after the flags, in kernel order.
  assert position(lines, "highdma: on") > position(lines, "receive-hashing: off [fixed]")
  assert "rx-checksumming: on" in ran[1].stdout.lines()
  assert "loopback: on [fixed]" in ran[2].stdout.lines()
  assert ran[3].status == 1
  assert ran[3].stderr == "netlink error: no device matches name (offset 24)\nnetlink error: No such device\n"
  assert ran[4].status == 1 and ran[4].stderr == "ethtool: unexpected parameter 'extra'\n"
}

test test_feature_changes_report_what_actually_changed { |ctx|
  let ran = session(ctx, [
    "-K d0 tx-nocache-copy on", "-k d0", "-K d0 tx-nocache-copy on",
    "-K d0 tx off", "-K d0 tso on", "-K d0 loopback on", "-K d0 rx on", "-K d0 sg off",
    "-K d0 tx-checksum-ipv4 off", "-K d0 gro off gro on",
  ])?
  guard ! ran.is_empty() else { return }
  # A request that goes as asked says nothing.
  assert ran[0].status == 0 and ran[0].stdout == "" and ran[0].stderr == ""
  assert "tx-nocache-copy: on" in ran[1].stdout.lines()
  assert ran[2].status == 0 and ran[2].stdout == ""
  # Turning checksumming off also turns off what depends on it.
  assert ran[3].status == 0
  let changes = ran[3].stdout.lines()
  assert changes[0] == "Actual changes:"
  assert "tx-checksum-ip-generic: off" in changes
  assert "tx-tcp-segmentation: off [not requested]" in changes
  # Wanting a feature that its dependencies hold off is reported, not an error.
  assert ran[4].status == 0
  assert ran[4].stdout.lines()[0] == "Actual changes:"
  assert "tx-tcp-segmentation: off [requested on]" in ran[4].stdout.lines()
  # A feature the device cannot change is refused with status 1.
  assert ran[5].status == 1
  assert ran[5].stdout == "Actual changes:\nloopback: off [requested on]\n", ran[5].stdout
  assert ran[5].stderr == "Could not change any device features\n"
  assert ran[6].status == 1
  assert ran[6].stdout == "Actual changes:\nrx-checksum: off [requested on]\n", ran[6].stdout
  # Naming a flag by its short word changes every feature it covers.
  assert ran[7].status == 0
  assert "tx-scatter-gather-fraglist: off" in ran[7].stdout.lines()
  # Requesting the state a fixed feature is already in changes nothing.
  assert ran[8].status == 0 and ran[8].stdout == "" and ran[8].stderr == ""
  assert ran[9].status == 0 and ran[9].stdout == ""
}

test test_feature_request_errors_follow_the_reference { |ctx|
  let ran = session(ctx, [
    "-K d0", "-K d0 nosuchfeature on", "-K d0 tx-nocache-copy on nosuch on", "-K d0 sg off nosuch on",
    "-K d0 tx", "-K d0 tx foo", "-K d0 foo", "-K d0 ufo on", "-K nosuch0 tx off", "-K d0 sg on -- tx off",
  ])?
  guard ! ran.is_empty() else { return }
  assert ran[0].status == 1 and ran[0].stderr == "Could not change any device features\n"
  for index in [1, 2, 3] {
    assert ran[index].status == 92
    assert ran[index].stdout == ""
  }
  let unsupported = ran[1].stderr.lines()
  assert unsupported[0] == "netlink error: bit name not found (offset 52)"
  assert not_supported(unsupported[1]), unsupported[1]
  # The offset is that of the unknown bit within the request message.
  assert ran[2].stderr.lines()[0] == "netlink error: bit name not found (offset 80)"
  assert ran[3].stderr.lines()[0] == "netlink error: bit name not found (offset 116)"
  assert ran[4].stderr == "ethtool (-K): flag '(null)' for parameter '(null)' is not followed by 'on' or 'off'\n"
  assert ran[5].stderr == "ethtool (-K): flag 'foo' for parameter '(null)' is not followed by 'on' or 'off'\n"
  assert ran[6].status == 1
  # A flag the kernel has no feature for expands to nothing.
  assert ran[7].status == 1 and ran[7].stderr == "Could not change any device features\n"
  assert ran[8].status == 92
  assert ran[8].stderr.starts_with("netlink error: no device matches name (offset 24)\n")
  # Words after `--` end the request.
  assert ran[9].status == 0
}

test test_channels_are_read_and_changed_on_a_veth_pair { |ctx|
  let ran = session(ctx, ["-l v0", "-L v0 rx 2", "-l v0", "-L v0", "-L v0 rx 99", "-L v0 rx 0 tx 0", "-L v0 other 1", "-L v0 rx", "-L v0 bogus 1", "-L v0 rx x", "-l d0"])?
  guard ! ran.is_empty() else { return }
  assert ran[0].stdout.lines() == [
    "Channel parameters for v0:",
    "Pre-set maximums:",
    "RX:\t\t32",
    "TX:\t\t32",
    "Other:\t\tn/a",
    "Combined:\tn/a",
    "Current hardware settings:",
    "RX:\t\t1",
    "TX:\t\t1",
    "Other:\t\tn/a",
    "Combined:\tn/a",
  ], ran[0].stdout
  assert ran[1].status == 0 and ran[1].stdout == "" and ran[1].stderr == ""
  assert "RX:\t\t2" in ran[2].stdout.lines()
  assert ran[3].status == 0 and ran[3].stdout == ""
  assert ran[4].status == 1
  assert ran[4].stderr.lines()[0] == "netlink error: requested channel count exceeds maximum (offset 32)"
  assert ran[4].stderr.lines()[1] == "netlink error: Invalid argument"
  assert ran[5].stderr.lines()[0] == "netlink error: requested channel counts would result in no RX or TX channel being configured (offset 32)"
  assert ran[6].stderr.lines()[0] == "netlink error: requested channel count exceeds maximum (offset 32)"
  assert ran[7].stderr == "ethtool (-L): no value for parameter 'rx'\n"
  assert ran[8].stderr == "ethtool (-L): unknown parameter 'bogus'\n"
  assert ran[9].stderr == "ethtool (-L): invalid value 'x' for parameter 'rx'\n"
  assert ran[10].status == 1 and ran[10].stdout == ""
  assert not_supported(ran[10].stderr.trim().lines()[0].replace("netlink error: ", with: "")), ran[10].stderr
}

test test_statistics_names_and_values_come_from_the_driver { |ctx|
  let ran = session(ctx, ["-S v0", "-S d0", "-S nosuch0", "-S v0 --all-groups", "-S v0 zzz", "--statistics v1"])?
  guard ! ran.is_empty() else { return }
  let rows = ran[0].stdout.lines()
  assert rows[0] == "NIC statistics:"
  assert rows[1].starts_with("     peer_ifindex: ")
  assert "     rx_queue_0_xdp_packets: 0" in rows
  assert ran[1].status == 94 and ran[1].stderr == "no stats available\n"
  assert ran[2].status == 96
  assert ran[2].stderr == "Cannot get stats strings information: No such device\n"
  assert ran[3].status == 1 and "--all-groups" in ran[3].stderr
  assert ran[4].stderr == "ethtool (-S): unknown parameter 'zzz'\n"
  assert ran[5].stdout.lines()[0] == "NIC statistics:"
}

test test_unsupported_operations_report_the_kernel_reason { |ctx|
  let ran = session(ctx, [
    "-g d0", "-c d0", "-a d0", "--show-eee d0", "-G d0 rx 1", "-C d0 rx-usecs 1", "-A d0 rx on",
    "--set-eee d0 eee on", "-s d0 speed 100", "-s d0 autoneg on", "-s d0 msglvl 7", "-s v0 duplex full",
    "-s lo speed 10", "-s nosuch0 speed 10", "-s v0", "-t d0", "-t nosuch0", "-t d0 offline extra",
  ])?
  guard ! ran.is_empty() else { return }
  let expected = [1, 1, 1, 1, 81, 1, 76, 76, 75, 75, 75, 75, 75, 75]
  for index in range(expected.len()) {
    assert ran[index].status == expected[index], f"{index}: {ran[index].status} {ran[index].stderr}"
    assert ran[index].stdout == ""
  }
  for index in range(8) {
    assert ran[index].stderr.starts_with("netlink error: ")
    assert not_supported(ran[index].stderr.trim()), ran[index].stderr
  }
  assert ran[9].stderr.lines()[0] == "netlink error: failed to retrieve link settings"
  assert ran[13].stderr == "netlink error: no device matches name (offset 24)\nnetlink error: No such device\n"
  assert ran[14].status == 0 and ran[14].stderr == ""
  assert ran[15].status == 74 and ran[15].stderr.starts_with("Cannot test: ")
  assert ran[16].status == 74
  assert ran[17].status == 1 and ran[17].stderr.starts_with("ethtool: bad command line argument(s)")
}

test test_permanent_address_and_time_stamping { |ctx|
  let ran = session(ctx, ["-P d0", "-P v0", "-P nosuch0", "-T lo", "-T v0", "-T nosuch0", "-T v0 foo"])?
  guard ! ran.is_empty() else { return }
  assert ran[0].stdout == "Permanent address: not set\n"
  assert ran[1].stdout == "Permanent address: not set\n"
  assert ran[2].status == 1 and ran[2].stderr == "netlink error: No such device\n"
  assert ran[3].stdout.lines() == [
    "Time stamping parameters for lo:",
    "Capabilities:",
    "\tsoftware-transmit",
    "\tsoftware-receive",
    "\tsoftware-system-clock",
    "PTP Hardware Clock: none",
    "Hardware Transmit Timestamp Modes: none",
    "Hardware Receive Filter Modes: none",
  ], ran[3].stdout
  assert ran[4].stdout.lines()[0] == "Time stamping parameters for v0:"
  assert ran[5].status == 1
  assert ran[6].stderr == "ethtool (-T): unknown parameter 'foo'\n"
}

test test_command_line_forms_and_json { |ctx|
  let ran = session(ctx, [
    "--version", "-h", "--driver d0", "--show-features d0", "--offload d0 tx-nocache-copy on", "-z d0",
    "--json -k v0", "--json -i d0", "-d d0", "--disable-netlink d0", "-I -S v0", "--json",
  ])?
  guard ! ran.is_empty() else { return }
  assert ran[0].stdout == "ethtool version 7.1\n"
  assert ran[1].status == 0 and ran[1].stdout.lines()[0] == "ethtool version 7.1"
  assert "        ethtool [ FLAGS ] -k|--show-features|--show-offload DEVNAME\tGet state of protocol offload and other features" in ran[1].stdout.lines()
  assert ran[2].stdout.lines()[0] == "driver: dummy"
  assert ran[3].stdout.lines()[0] == "Features for d0:"
  assert ran[4].status == 0 and ran[4].stdout == ""
  assert ran[5].stderr == "ethtool: bad command line argument(s)\nFor more information run ethtool -h\n"
  let document = json.decode(ran[6].stdout)?
  assert json.get(document, [0, "ifname"])?.require(Str)? == "v0"
  assert json.get(document, [0, "rx-checksumming", "active"])?.require(Bool)?
  assert json.get(document, [0, "tx-checksumming", "fixed"])? == null
  assert json.get(document, [0, "tx-checksum-ipv4", "fixed"])?.require(Bool)?
  assert ran[6].stdout.starts_with("[ {\n        \"ifname\": \"v0\",\n")
  assert ran[6].stdout.ends_with("\n    } ]\n")
  assert ran[7].status == 1
  assert ran[7].stderr == "ethtool: bad command line argument(s)\nJSON output not available for this subcommand\nFor more information run ethtool -h\n"
  assert ran[8].status == 1
  assert ran[8].stderr == "ethtool: -d is not supported: register dumps need per-driver decoders\n"
  assert ran[9].status == 1 and "--disable-netlink is not supported" in ran[9].stderr
  assert ran[10].status == 1 and "-I is not supported" in ran[10].stderr
  assert ran[11].status == 1
}

# Pure layout tests: no kernel needed.

test test_feature_names_expand_flags_to_kernel_features {
  let known = ["tx-scatter-gather", "tx-checksum-ipv4", "", "tx-checksum-ip-generic", "tx-scatter-gather-fraglist", "tx-tcp-segmentation", "tx-tcp6-segmentation", "tx-gso-list", "rx-checksum"]
  assert dec.feature_names("tx-gso-list", known) == ["tx-gso-list"]
  assert dec.feature_names("tx", known) == ["tx-checksum-ipv4", "tx-checksum-ip-generic"]
  assert dec.feature_names("tx-checksumming", known) == ["tx-checksum-ipv4", "tx-checksum-ip-generic"]
  assert dec.feature_names("sg", known) == ["tx-scatter-gather", "tx-scatter-gather-fraglist"]
  assert dec.feature_names("tso", known) == ["tx-tcp-segmentation", "tx-tcp6-segmentation"]
  assert dec.feature_names("rx", known) == ["rx-checksum"]
  assert dec.feature_names("ufo", known) == []
  assert dec.feature_names("no-such-feature", known) == []
  assert dec.is_flag("ufo") and dec.is_flag("scatter-gather") and ! dec.is_flag("tx-gso-list")
}

test test_feature_report_orders_flags_before_loose_features {
  let features: List[dec.Feature] = [
    {index: 0, name: "tx-scatter-gather", requested: true, active: true, fixed: false},
    {index: 1, name: "tx-checksum-ipv4", requested: false, active: false, fixed: true},
    {index: 3, name: "tx-checksum-ip-generic", requested: true, active: false, fixed: false},
    {index: 5, name: "highdma", requested: true, active: true, fixed: false},
    {index: 6, name: "tx-scatter-gather-fraglist", requested: true, active: true, fixed: false},
    {index: 40, name: "rx-checksum", requested: false, active: true, fixed: true},
    {index: 41, name: "tx-nocache-copy", requested: false, active: false, fixed: false},
  ]
  assert dec.features_lines("eth9", features) == [
    "Features for eth9:",
    "rx-checksumming: on [fixed]",
    "tx-checksumming: off",
    "\ttx-checksum-ipv4: off [fixed]",
    "\ttx-checksum-ip-generic: off [requested on]",
    "scatter-gather: on",
    "\ttx-scatter-gather: on",
    "\ttx-scatter-gather-fraglist: on",
    "highdma: on",
    "tx-nocache-copy: off",
  ]
  let document = json.decode(dec.features_json("eth9", features))?
  assert json.get(document, [0, "tx-checksumming", "active"])?.require(Bool)? == false
  assert json.get(document, [0, "tx-checksum-ip-generic", "requested"])?.require(Bool)?
}

test test_feature_outcome_names_only_surprises {
  let before: List[dec.Feature] = [
    {index: 0, name: "tx-scatter-gather", requested: true, active: true, fixed: false},
    {index: 1, name: "tx-generic-segmentation", requested: true, active: true, fixed: false},
    {index: 2, name: "loopback", requested: false, active: false, fixed: true},
  ]
  var asked: Map[Int, Bool] = {}
  asked = asked.set(0, false)
  let quiet: List[dec.Feature] = [
    {index: 0, name: "tx-scatter-gather", requested: false, active: false, fixed: false},
    {index: 1, name: "tx-generic-segmentation", requested: true, active: true, fixed: false},
    {index: 2, name: "loopback", requested: false, active: false, fixed: true},
  ]
  let ordinary = dec.features_outcome(before, quiet, asked)
  assert ordinary.lines == [] and ordinary.changed and ! ordinary.unsatisfied
  var chained = quiet
  chained[1] = {index: 1, name: "tx-generic-segmentation", requested: true, active: false, fixed: false}
  let surprise = dec.features_outcome(before, chained, asked)
  assert surprise.lines == ["tx-scatter-gather: off", "tx-generic-segmentation: off [not requested]"]
  var refused: Map[Int, Bool] = {}
  refused = refused.set(2, true)
  let denied = dec.features_outcome(before, before, refused)
  assert denied.lines == ["loopback: off [requested on]"]
  assert ! denied.changed and denied.unsatisfied
}

test test_settings_report_lists_modes_and_partner_when_present {
  # 10baseT/Half and /Full, 1000baseT/Full, TP, autonegotiation, symmetric
  # pause; the partner advertises 100baseT/Full and pause.
  let supported = 1 + 2 + 32 + 64 + 128 + 8192
  let advertising = 1 + 2 + 32 + 64 + 8192
  let partner = 8 + 64 + 8192
  let settings: dec.LinkSettings = {
    speed: 1000, duplex: 1, port: 0, phy: 1, autoneg: 1, mdix: 2, mdix_ctrl: 3, transceiver: 0,
    supported: [supported, 0, 0], advertising: [advertising, 0, 0], partner: [partner, 0, 0], nwords: 3, raw: b"",
  }
  assert dec.settings_lines(settings) == [
    "\tSupported ports: [ TP ]",
    "\tSupported link modes:   10baseT/Half 10baseT/Full ",
    "\t                        1000baseT/Full ",
    "\tSupported pause frame use: Symmetric",
    "\tSupports auto-negotiation: Yes",
    "\tSupported FEC modes: Not reported",
    "\tAdvertised link modes:  10baseT/Half 10baseT/Full ",
    "\t                        1000baseT/Full ",
    "\tAdvertised pause frame use: Symmetric",
    "\tAdvertised auto-negotiation: Yes",
    "\tAdvertised FEC modes: Not reported",
    "\tLink partner advertised link modes:  100baseT/Full ",
    "\tLink partner advertised pause frame use: Symmetric",
    "\tLink partner advertised auto-negotiation: Yes",
    "\tLink partner advertised FEC modes: Not reported",
    "\tSpeed: 1000Mb/s",
    "\tDuplex: Full",
    "\tAuto-negotiation: on",
    "\tPort: Twisted Pair",
    "\tPHYAD: 1",
    "\tTransceiver: internal",
    "\tMDI-X: on (auto)",
  ]
}

test test_link_settings_requests_carry_the_handshake_and_round_trip {
  assert dec.link_settings_request(0)?.len() == 48
  assert dec.link_settings_request(3)?.len() == 48 + 36
  # The kernel answers the first request with the word count it wants, negated.
  var reply = bytes.concat([bytes.pack_le(76, 4)?, bytes.zero(11)?, b"\xfd", bytes.zero(32)?])
  assert dec.link_settings_nwords(reply) == -3
  reply = bytes.concat([
    bytes.pack_le(76, 4)?, bytes.pack_le(100, 4)?, bytes.from_ints([1, 0, 7, 1, 0, 0, 0, 3, 0, 0, 0, 0])?, bytes.zero(28)?,
    bytes.pack_le(5, 4)?, bytes.pack_le(0, 4)?, bytes.pack_le(0, 4)?, bytes.pack_le(1, 4)?, bytes.pack_le(0, 4)?, bytes.pack_le(0, 4)?,
    bytes.pack_le(0, 4)?, bytes.pack_le(0, 4)?, bytes.pack_le(0, 4)?,
  ])
  let settings = dec.link_settings_decode(reply)?
  assert settings.speed == 100 and settings.duplex == 1 and settings.phy == 7 and settings.autoneg == 1
  assert settings.supported == [5, 0, 0] and settings.advertising == [1, 0, 0] and settings.nwords == 3
  let request = dec.link_settings_set_request(settings, 10, null, null, null, 0, null, [1, 0, 0])?
  assert bytes.unpack_le(request, 4, 0)? == 77
  assert bytes.unpack_le(request, 4, 4)? == 10
  assert request.byte_at(11) == 0
  assert request.byte_at(15) == 3
  assert request.len() == reply.len()
}

test test_wake_on_lan_and_message_level_text {
  assert dec.wol_letters(0) == "d"
  assert dec.wol_letters(1 + 2 + 4 + 8 + 32) == "pumbg"
  assert dec.wol_parse("pg") == 33
  assert dec.wol_parse("d") == 0
  assert dec.wol_parse("x") == null
  let reply = bytes.concat([bytes.pack_le(5, 4)?, bytes.pack_le(63, 4)?, bytes.pack_le(32, 4)?, bytes.zero(8)?])
  assert dec.wol_lines(reply)? == ["\tSupports Wake-on: pumbag", "\tWake-on: g"]
  assert dec.msglvl_lines(7) == ["\tCurrent message level: 0x00000007 (7)", "\t\t\t       drv probe link"]
  assert dec.msglvl_lines(0) == ["\tCurrent message level: 0x00000000 (0)", "\t\t\t       "]
}

test test_ring_pause_and_coalesce_text {
  let ring = dec.words(16, [4096, 0, 0, 4096, 256, 0, 0, 128])?
  assert dec.ring_lines("eth9", ring)? == [
    "Ring parameters for eth9:", "Pre-set maximums:", "RX:\t\t\t4096", "RX Mini:\t\tn/a", "RX Jumbo:\t\tn/a", "TX:\t\t\t4096",
    "Current hardware settings:", "RX:\t\t\t256", "RX Mini:\t\tn/a", "RX Jumbo:\t\tn/a", "TX:\t\t\t128", "",
  ]
  assert dec.pause_lines("eth9", dec.words(18, [1, 0, 1])?)? == [
    "Pause parameters for eth9:", "Autonegotiate:\ton", "RX:\t\toff", "TX:\t\ton", "",
  ]
  let request = dec.pause_set_request(dec.words(18, [1, 0, 1])?, null, true, null)?
  assert dec.unwords(request, 3)? == [1, 1, 1]
  assert bytes.unpack_le(request, 4, 0)? == 19
  let coalesce = dec.words(14, [10, 20, 0, 0, 30, 40, 0, 0, 5, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 7])?
  let lines = dec.coalesce_lines("eth9", coalesce)?
  assert lines[1] == "Adaptive RX: on  TX: off"
  assert "rx-usecs:\t10" in lines and "tx-frames:\t40" in lines and "sample-interval:\t7" in lines
  var changes: Map[Int, Int] = {}
  changes = changes.set(0, 99)
  assert dec.unwords(dec.coalesce_set_request(coalesce, changes)?, 3)? == [99, 20, 0]
  assert dec.COALESCE_FIELDS[0] == "rx-usecs" and dec.COALESCE_FIELDS[21] == "sample-interval"
}

test test_channel_refusals_name_the_offending_attribute {
  let current = dec.words(60, [8, 8, 0, 4, 2, 2, 0, 0])?
  let over = dec.channels_refusal(current, [9, null, null, null], 2)?
  assert over == "requested channel count exceeds maximum (offset 32)"
  let second = dec.channels_refusal(current, [1, 9, null, null], 2)?
  assert second == "requested channel count exceeds maximum (offset 40)"
  let none = dec.channels_refusal(current, [0, null, null, 0], 2)?
  assert none == "requested channel counts would result in no RX or TX channel being configured (offset 32)"
  assert dec.channels_refusal(current, [1, 1, null, null], 2)? == null
  let request = dec.channels_set_request(current, [null, 5, null, 1])?
  assert dec.unwords(request, 8)? == [8, 8, 0, 4, 2, 5, 0, 1]
  assert bytes.unpack_le(request, 4, 0)? == 61
}

test test_driver_and_hardware_address_decoding {
  var reply = bytes.concat([
    bytes.pack_le(3, 4)?, bytes.from_text("veth"), bytes.zero(28)?, bytes.from_text("1.0"), bytes.zero(29)?,
    bytes.from_text("fw 9"), bytes.zero(28)?, bytes.from_text("0000:00:03.0"), bytes.zero(20)?, bytes.zero(32)?, bytes.zero(12)?,
    bytes.pack_le(0, 4)?, bytes.pack_le(21, 4)?, bytes.pack_le(0, 4)?, bytes.pack_le(256, 4)?, bytes.pack_le(0, 4)?,
  ])
  let info = dec.drvinfo_decode(reply)?
  assert info.driver == "veth" and info.firmware == "fw 9" and info.bus == "0000:00:03.0"
  assert dec.drvinfo_lines(info)[5..] == [
    "supports-statistics: yes", "supports-test: no", "supports-eeprom-access: yes", "supports-register-dump: no", "supports-priv-flags: no",
  ]
  let address = bytes.concat([bytes.pack_le(32, 4)?, bytes.pack_le(6, 4)?, bytes.from_ints([82, 84, 0, 18, 52, 86])?, bytes.zero(26)?])
  assert dec.permaddr_line(address)? == "Permanent address: 52:54:00:12:34:56"
}

test test_time_stamping_report_lists_hardware_modes {
  let reply = dec.words(65, [1 + 2 + 4 + 8 + 16 + 64, 3, 3, 0, 0, 0, 1 + 2 + 8, 0, 0, 0])?
  assert dec.tsinfo_lines("eth9", reply)? == [
    "Time stamping parameters for eth9:", "Capabilities:",
    "\thardware-transmit", "\tsoftware-transmit", "\thardware-receive", "\tsoftware-receive", "\tsoftware-system-clock", "\thardware-raw-clock",
    "PTP Hardware Clock: 3",
    "Hardware Transmit Timestamp Modes:",
    "\toff                   (HWTSTAMP_TX_OFF)",
    "\ton                    (HWTSTAMP_TX_ON)",
    "Hardware Receive Filter Modes:",
    "\tnone                  (HWTSTAMP_FILTER_NONE)",
    "\tall                   (HWTSTAMP_FILTER_ALL)",
    "\tptpv1-l4-event        (HWTSTAMP_FILTER_PTP_V1_L4_EVENT)",
  ]
}
