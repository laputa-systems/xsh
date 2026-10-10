type Ran = {status: Int, stdout: Str, stderr: Str, elapsed_ms: Int}

# Every probe stays on this host's loopback: the destination is 127.0.0.1 or
# ::1, a hop is the destination itself, and the kernel answers a probe to a
# closed port with ICMP port unreachable from the same address.

# Runs core/traceroute.xsh by its real path so lib.gnu resolves beside it,
# capturing both streams and the wall time.
proc traceroute(ctx: TestContext, args: List[Str]) [fs, process, time, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "traceroute")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/traceroute.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, b"", out, err)
  let started = time.now()
  let status = process.run(plan)?
  let elapsed = time.now() - started

  Ok({status: status.exit_code()?, stdout: out.read_text()?, stderr: err.read_text()?, elapsed_ms: elapsed})
}

proc open_udp(family: Str, address: Str, port: Int) [process, error] -> Result[Int] {
  let c = linux.net_constants()
  let fd = linux.socket(if family == "inet6" { c.AF_INET6 } else { c.AF_INET }, c.SOCK_DGRAM)?

  linux.bind(fd, {family: family, address: address, port: port})?

  Ok(fd)
}

proc local_port(fd: Int) [process, error] -> Result[Int] {
  Ok(linux.getsockname(fd)?.port)
}

# The datagrams waiting on `fd` after a trace finished, in arrival order.
proc drain(fd: Int) [process, error] -> Result[List[Bytes]] {
  let c = linux.net_constants()
  var found: List[Bytes] = []

  while "readable" in unix.poll_fd(fd, ["readable"], 200)? {
    found += [linux.recvfrom(fd, 70000, c.MSG_DONTWAIT)?.data]
  }

  Ok(found)
}

proc assert_hop(line: Str, index: Int, address: Str, probes: Int) [error] -> Result[Unit] {
  var pattern = f"^{if index < 10 { " " } else { "" }}{index}  {address}"

  for _ in range(probes) {
    pattern = f"{pattern}  [0-9]+\\.[0-9]{{3}} ms"
  }

  assert regex.compile(f"{pattern}$")?.matches(line), f"hop line {line} does not match {pattern}"
  Ok()
}

test test_traceroute_default_udp_reaches_loopback_in_one_hop { |ctx|
  let ran = traceroute(ctx, ["-n", "127.0.0.1"])?
  let lines = ran.stdout.lines()

  assert ran.status == 0, ran.stderr
  assert ran.stderr == ""
  assert lines.len() == 2, ran.stdout
  assert lines[0] == "traceroute to 127.0.0.1 (127.0.0.1), 30 hops max, 60 byte packets"
  assert_hop(lines[1], 1, "127\\.0\\.0\\.1", 3)?
  assert ran.stdout.ends_with("\n")
}

test test_traceroute_prints_names_unless_numeric { |ctx|
  let named = traceroute(ctx, ["-q1", "127.0.0.1"])?
  let hop = named.stdout.lines()[1]

  assert named.status == 0, named.stderr
  assert regex.compile("^ 1  [^ ]+ \\(127\\.0\\.0\\.1\\)  [0-9]+\\.[0-9]{3} ms$")?.matches(hop), hop

  let numeric = traceroute(ctx, ["-n", "-q1", "127.0.0.1"])?
  assert_hop(numeric.stdout.lines()[1], 1, "127\\.0\\.0\\.1", 1)?
}

test test_traceroute_icmp_echo_reaches_loopback { |ctx|
  let ran = traceroute(ctx, ["-I", "-n", "127.0.0.1"])?

  if ran.status != 0 {
    assert ran.stderr.find("socket:") != null, ran.stderr
    test.skip(f"no ICMP socket is permitted here: {ran.stderr.trim()}")
    return
  }

  let lines = ran.stdout.lines()
  assert lines[0] == "traceroute to 127.0.0.1 (127.0.0.1), 30 hops max, 60 byte packets"
  assert lines.len() == 2, ran.stdout
  assert_hop(lines[1], 1, "127\\.0\\.0\\.1", 3)?

  let by_module = traceroute(ctx, ["-M", "icmp", "-n", "-q1", "127.0.0.1"])?
  assert by_module.status == 0, by_module.stderr
  assert_hop(by_module.stdout.lines()[1], 1, "127\\.0\\.0\\.1", 1)?
}

test test_traceroute_icmp_socket_choice_is_honored { |ctx|
  let datagram = traceroute(ctx, ["-I", "-O", "dgram", "-n", "-q1", "127.0.0.1"])?

  if datagram.status == 0 {
    assert_hop(datagram.stdout.lines()[1], 1, "127\\.0\\.0\\.1", 1)?
  } else {
    assert datagram.stderr.find("socket:") != null, datagram.stderr
  }

  let raw = traceroute(ctx, ["-I", "-O", "raw", "-n", "-q1", "127.0.0.1"])?

  if raw.status == 0 {
    assert_hop(raw.stdout.lines()[1], 1, "127\\.0\\.0\\.1", 1)?
  } else {
    assert raw.status == 1, raw.stderr
    assert raw.stderr.find("socket:") != null, raw.stderr
    assert raw.stdout.ends_with("byte packets"), "the failure follows the unterminated header"
  }

  let both = traceroute(ctx, ["-I", "-O", "raw,dgram", "-n", "127.0.0.1"])?
  assert both.status == 0 or both.status == 1
}

test test_traceroute_ipv6_loopback { |ctx|
  match open_udp("inet6", "::1", 0) {
    Err(failure) => {
      test.skip(f"IPv6 loopback is unavailable: {failure.message}")
      return
    }
    Ok(probe) => {
      unix.close_fd(probe)?
    }
  }

  let ran = traceroute(ctx, ["-6", "-n", "::1"])?
  let lines = ran.stdout.lines()

  assert ran.status == 0, ran.stderr
  assert lines[0] == "traceroute to ::1 (::1), 30 hops max, 80 byte packets"
  assert_hop(lines[1], 1, "::1", 3)?

  let echo = traceroute(ctx, ["-6", "-I", "-n", "-q1", "::1"])?

  if echo.status == 0 {
    assert_hop(echo.stdout.lines()[1], 1, "::1", 1)?
  } else {
    assert echo.stderr.find("socket:") != null, echo.stderr
  }

  let minimum = traceroute(ctx, ["-6", "-n", "-q1", "::1", "10"])?
  assert minimum.stdout.lines()[0] == "traceroute to ::1 (::1), 30 hops max, 48 byte packets"
}

test test_traceroute_address_family_flags_follow_the_literal { |ctx|
  let v6_only = traceroute(ctx, ["-4", "::1"])?
  assert v6_only.status == 2
  assert v6_only.stdout == ""
  assert v6_only.stderr == "::1: Name has no usable address\nCannot handle \"host\" cmdline arg `::1' on position 1 (argc 2)\n", v6_only.stderr

  let v4_only = traceroute(ctx, ["-6", "127.0.0.1"])?
  assert v4_only.status == 2
  assert v4_only.stderr.starts_with("127.0.0.1: Name has no usable address\n"), v4_only.stderr

  let last_wins = traceroute(ctx, ["-6", "-4", "-n", "-q1", "127.0.0.1"])?
  assert last_wins.status == 0, last_wins.stderr
}

test test_traceroute_hop_range_and_probe_count { |ctx|
  let ran = traceroute(ctx, ["-n", "-q1", "-m3", "-f2", "127.0.0.1"])?
  let lines = ran.stdout.lines()

  assert ran.status == 0, ran.stderr
  assert lines.len() == 2, ran.stdout
  assert lines[0] == "traceroute to 127.0.0.1 (127.0.0.1), 3 hops max, 60 byte packets"
  assert_hop(lines[1], 2, "127\\.0\\.0\\.1", 1)?

  let long_form = traceroute(ctx, ["-n", "--queries=2", "--max-hops", "4", "--first=3", "127.0.0.1"])?
  assert long_form.status == 0, long_form.stderr
  assert long_form.stdout.lines()[0] == "traceroute to 127.0.0.1 (127.0.0.1), 4 hops max, 60 byte packets"
  assert_hop(long_form.stdout.lines()[1], 3, "127\\.0\\.0\\.1", 2)?

  let ten = traceroute(ctx, ["-n", "-q10", "127.0.0.1"])?
  assert ten.status == 0, ten.stderr
  assert_hop(ten.stdout.lines()[1], 1, "127\\.0\\.0\\.1", 10)?
}

test test_traceroute_packet_length_sets_the_payload { |ctx|
  let listener = open_udp("inet", "127.0.0.1", 0)?
  defer unix.close_fd(listener)
  let port = local_port(listener)?
  let target = ["-n", "-q1", "-m1", "-w", "0.2", "-p", f"{port}", "127.0.0.1"]

  let normal = traceroute(ctx, target)?
  assert normal.stdout.lines()[0] == "traceroute to 127.0.0.1 (127.0.0.1), 1 hops max, 60 byte packets"
  assert normal.stdout.lines()[1] == " 1  *", normal.stdout

  let first = drain(listener)?
  assert first.len() == 1
  assert first[0].len() == 32
  assert first[0].byte_at(0) == 64 and first[0].byte_at(31) == 95

  let longer = traceroute(ctx, target.extend(["100"]))?
  assert longer.stdout.lines()[0] == "traceroute to 127.0.0.1 (127.0.0.1), 1 hops max, 100 byte packets"
  assert drain(listener)?[0].len() == 72

  let minimum = traceroute(ctx, target.extend(["20"]))?
  assert minimum.stdout.lines()[0] == "traceroute to 127.0.0.1 (127.0.0.1), 1 hops max, 28 byte packets"
  assert drain(listener)?[0].len() == 0
}

test test_traceroute_udp_port_advances_per_probe_unless_fixed { |ctx|
  let first = open_udp("inet", "127.0.0.1", 0)?
  defer unix.close_fd(first)
  let port = local_port(first)?

  match open_udp("inet", "127.0.0.1", port + 1) {
    Err(failure) => {
      test.skip(f"the next port is not free: {failure.message}")
      return
    }
    Ok(second) => {
      defer unix.close_fd(second)

      let stepped = traceroute(ctx, ["-n", "-q2", "-m1", "-w", "0.2", "-p", f"{port}", "127.0.0.1"])?
      assert stepped.stdout.lines()[1] == " 1  * *", stepped.stdout
      assert drain(first)?.len() == 1
      assert drain(second)?.len() == 1

      let fixed = traceroute(ctx, ["-n", "-U", "-q2", "-m1", "-w", "0.2", "-p", f"{port}", "127.0.0.1"])?
      assert fixed.stdout.lines()[1] == " 1  * *", fixed.stdout
      assert drain(first)?.len() == 2
      assert drain(second)?.len() == 0

      let by_module = traceroute(ctx, ["-n", "-M", "udp", "-q2", "-m1", "-w", "0.2", "-p", f"{port}", "127.0.0.1"])?
      assert by_module.stdout.lines()[1] == " 1  * *", by_module.stdout
      assert drain(first)?.len() == 2
    }
  }
}

test test_traceroute_applies_source_tos_ttl_and_port { |ctx|
  let c = linux.net_constants()
  let listener = open_udp("inet", "127.0.0.1", 0)?
  defer unix.close_fd(listener)
  linux.setsockopt_int(listener, c.SOL_IP, c.IP_RECVTTL, 1)?
  linux.setsockopt_int(listener, c.SOL_IP, c.IP_RECVTOS, 1)?

  let spare = open_udp("inet", "127.0.0.1", 0)?
  let source_port = local_port(spare)?
  unix.close_fd(spare)?

  let ran = traceroute(
    ctx,
    ["-n", "-q1", "-f5", "-m5", "-w", "0.2", "-s", "127.0.0.2", "--sport", f"{source_port}", "-t", "16", "-p", f"{local_port(listener)?}", "127.0.0.1"],
  )?
  assert ran.status == 0, ran.stderr

  assert "readable" in unix.poll_fd(listener, ["readable"], 1000)?
  let got = linux.recvfrom(listener, 2048)?
  assert got.address.address == "127.0.0.2"
  assert got.address.port == source_port

  let ttl = [m for m in got.control if m.level == c.SOL_IP and m.type == c.IP_TTL]
  assert bytes.unpack_le(ttl[0].data, 4)? == 5
  let tos = [m for m in got.control if m.level == c.SOL_IP and m.type == c.IP_TOS]
  assert tos[0].data.byte_at(0) == 16
}

test test_traceroute_gateway_adds_a_source_route { |ctx|
  let ran = traceroute(ctx, ["-n", "-q1", "-m1", "-w", "0.2", "-g", "127.0.0.1", "127.0.0.1"])?
  assert ran.status == 0, ran.stderr
  assert ran.stdout.lines()[0] == "traceroute to 127.0.0.1 (127.0.0.1), 1 hops max, 72 byte packets"

  let two = traceroute(ctx, ["-n", "-q1", "-m1", "-w", "0.2", "--gateway=127.0.0.1,127.0.0.1", "-g", "127.0.0.1", "127.0.0.1"])?
  assert two.stdout.lines()[0] == "traceroute to 127.0.0.1 (127.0.0.1), 1 hops max, 80 byte packets"

  let nine = traceroute(ctx, ["-n", "-g", "127.0.0.1,127.0.0.1,127.0.0.1,127.0.0.1,127.0.0.1,127.0.0.1,127.0.0.1,127.0.0.1,127.0.0.1", "127.0.0.1"])?
  assert nine.status == 2
  assert nine.stderr == "Too many gateways specified (maximum 8 for IPv4)\n", nine.stderr

  let v6 = traceroute(ctx, ["-6", "-g", "::1", "::1"])?
  assert v6.status == 2
  assert v6.stderr.starts_with("traceroute: option '-g' is not supported"), v6.stderr
}

test test_traceroute_wait_and_sendwait_pace_the_probes { |ctx|
  let listener = open_udp("inet", "127.0.0.1", 0)?
  defer unix.close_fd(listener)
  let port = f"{local_port(listener)?}"

  let waited = traceroute(ctx, ["-n", "-U", "-q2", "-m1", "-w", "1", "-p", port, "127.0.0.1"])?
  assert waited.stdout.lines()[1] == " 1  * *", waited.stdout
  assert waited.elapsed_ms >= 1800, f"two one-second waits took {waited.elapsed_ms} ms"

  let seconds = traceroute(ctx, ["-n", "-U", "-q3", "-m1", "-w", "0.05", "-z", "0.4", "-p", port, "127.0.0.1"])?
  assert seconds.stdout.lines()[1] == " 1  * * *", seconds.stdout
  assert seconds.elapsed_ms >= 750, f"-z 0.4 spaced the probes by too little: {seconds.elapsed_ms} ms"

  let millis = traceroute(ctx, ["-n", "-U", "-q3", "-m1", "-w", "0.05", "-z", "400", "-p", port, "127.0.0.1"])?
  assert millis.elapsed_ms >= 750, f"-z 400 is milliseconds: {millis.elapsed_ms} ms"

  let shaped = traceroute(ctx, ["-n", "-U", "-q2", "-m1", "-w", "0.3,2,5", "-p", port, "127.0.0.1"])?
  assert shaped.status == 0, shaped.stderr
  assert shaped.stdout.lines()[1] == " 1  * *", shaped.stdout
}

test test_traceroute_tcp_syn_reaches_open_and_closed_ports { |ctx|
  let c = linux.net_constants()
  let server = linux.socket(c.AF_INET, c.SOCK_STREAM)?
  defer unix.close_fd(server)
  linux.bind(server, {family: "inet", address: "127.0.0.1", port: 0})?
  linux.listen(server, 4)?
  let open_port = linux.getsockname(server)?.port

  let ran = traceroute(ctx, ["-T", "-n", "-q2", "-p", f"{open_port}", "127.0.0.1"])?

  if ran.status != 0 {
    assert ran.status == 1
    assert ran.stderr == "You do not have enough privileges to use this traceroute method.\nsocket: Operation not permitted\n", ran.stderr
    assert ran.stdout == ""
    test.skip("TCP probes need a raw ICMP socket")
    return
  }

  assert ran.stdout.lines()[0] == "traceroute to 127.0.0.1 (127.0.0.1), 30 hops max, 60 byte packets"
  assert_hop(ran.stdout.lines()[1], 1, "127\\.0\\.0\\.1", 2)?

  let spare = linux.socket(c.AF_INET, c.SOCK_STREAM)?
  linux.bind(spare, {family: "inet", address: "127.0.0.1", port: 0})?
  let closed_port = linux.getsockname(spare)?.port
  unix.close_fd(spare)?

  let refused = traceroute(ctx, ["-T", "-n", "-q1", "-p", f"{closed_port}", "127.0.0.1"])?
  assert refused.status == 0, refused.stderr
  assert_hop(refused.stdout.lines()[1], 1, "127\\.0\\.0\\.1", 1)?

  let by_module = traceroute(ctx, ["-M", "tcp", "-n", "-q1", "-p", f"{open_port}", "127.0.0.1"])?
  assert by_module.status == 0, by_module.stderr
}

test test_traceroute_rejects_invalid_values { |ctx|
  let cases = [
    {args: ["-n"], text: "Specify \"host\" missing argument.\n"},
    {args: ["-q0", "127.0.0.1"], text: "no more than 10 probes per hop\n"},
    {args: ["-q11", "127.0.0.1"], text: "no more than 10 probes per hop\n"},
    {args: ["-m256", "127.0.0.1"], text: "max hops cannot be more than 255\n"},
    {args: ["-f0", "127.0.0.1"], text: "first hop out of range\n"},
    {args: ["-f5", "-m3", "127.0.0.1"], text: "first hop out of range\n"},
    {args: ["-w", "-1", "127.0.0.1"], text: "bad wait specifications `-1' used\n"},
    {args: ["-w", "x", "127.0.0.1"], text: "Cannot handle `-w' option with arg `x' (argc 3)\n"},
    {args: ["-z", "-1", "127.0.0.1"], text: "bad sendtime `-1' specified\n"},
    {args: ["-p", "70000", "127.0.0.1"], text: "Cannot handle `-p' option with arg `70000' (argc 3)\n"},
    {args: ["-t", "300", "127.0.0.1"], text: "Cannot handle `-t' option with arg `300' (argc 3)\n"},
    {args: ["-N", "x", "127.0.0.1"], text: "Cannot handle `-N' option with arg `x' (argc 3)\n"},
    {args: ["-s", "::1", "127.0.0.1"], text: "IP version mismatch in addresses specified\n"},
    {args: ["127.0.0.1", "abc"], text: "Cannot handle \"packetlen\" cmdline arg `abc' on position 2 (argc 2)\n"},
    {args: ["127.0.0.1", "70000"], text: "too big packetlen 70000 specified\n"},
    {args: ["127.0.0.1", "60", "3"], text: "Extra arg `3' (position 3, argc 3)\n"},
    {args: ["-M", "nosuch", "127.0.0.1"], text: "Unknown traceroute module nosuch\n"},
    {args: ["-O", "bogus", "127.0.0.1"], text: "Unknown option `bogus' for module `default'\n"},
    {args: ["-T", "127.0.0.1", "100"], text: "traceroute: PACKETLEN is not supported with -T: the probe is a bare SYN\n"},
  ]

  for entry in cases {
    let ran = traceroute(ctx, entry.args)?

    let shown = entry.args.join(" ")

    assert ran.status == 2, f"{shown}: {ran.status} {ran.stderr}"
    assert ran.stdout == "", f"{shown}: {ran.stdout}"
    assert ran.stderr == entry.text, f"{shown}: {ran.stderr}"
  }
}

test test_traceroute_refuses_unsupported_options_by_name { |ctx|
  for flag in ["-A", "-e", "-l", "-D", "--mtu", "--back", "--as-path-lookups", "--extensions", "--dccp"] {
    let args = if flag == "-l" { [flag, "1", "127.0.0.1"] } else { [flag, "127.0.0.1"] }
    let ran = traceroute(ctx, args)?

    assert ran.status == 2, f"{flag}: {ran.status}"
    assert ran.stdout == "", f"{flag}: {ran.stdout}"
    assert ran.stderr.starts_with(f"traceroute: option '{flag}' is not supported: "), f"{flag}: {ran.stderr}"
  }

  let protocol = traceroute(ctx, ["-P", "47", "127.0.0.1"])?
  assert protocol.status == 2
  assert protocol.stderr.starts_with("traceroute: option '-P' is not supported: "), protocol.stderr
}

test test_traceroute_reports_socket_setup_failures_after_the_header { |ctx|
  let device = traceroute(ctx, ["-n", "-i", "nosuch0", "127.0.0.1"])?
  assert device.status == 1
  assert device.stdout == "traceroute to 127.0.0.1 (127.0.0.1), 30 hops max, 60 byte packets"
  assert device.stderr == "\nsetsockopt SO_BINDTODEVICE: No such device\n", device.stderr

  let source = traceroute(ctx, ["-n", "-s", "192.0.2.1", "127.0.0.1"])?
  assert source.status == 1
  assert source.stdout.ends_with("byte packets")
  assert source.stderr == "\nbind: Address not available\n", source.stderr

  let loopback = traceroute(ctx, ["-n", "-q1", "-i", "lo", "127.0.0.1"])?
  assert loopback.status == 0, loopback.stderr
  assert_hop(loopback.stdout.lines()[1], 1, "127\\.0\\.0\\.1", 1)?
}

test test_traceroute_socket_flags_are_applied_or_refused_by_the_kernel { |ctx|
  let plain = traceroute(ctx, ["-n", "-q1", "-F", "-r", "127.0.0.1"])?
  assert plain.status == 0, plain.stderr
  assert_hop(plain.stdout.lines()[1], 1, "127\\.0\\.0\\.1", 1)?

  let debug = traceroute(ctx, ["-n", "-q1", "-d", "127.0.0.1"])?

  if debug.status != 0 {
    assert debug.stderr == "\nsetsockopt SO_DEBUG: Permission denied\n", debug.stderr
  }

  let mark = traceroute(ctx, ["-n", "-q1", "--fwmark=7", "127.0.0.1"])?

  if mark.status != 0 {
    assert mark.stderr == "\nsetsockopt SO_MARK: Operation not permitted\n", mark.stderr
  }

  let simultaneous = traceroute(ctx, ["-n", "-q1", "-N", "4", "127.0.0.1"])?
  assert simultaneous.status == 0, simultaneous.stderr
}

test test_traceroute_module_option_help { |ctx|
  let default = traceroute(ctx, ["-O", "help", "127.0.0.1"])?
  assert default.status == 0
  assert default.stdout == "No options for module `default'\n"

  let udp = traceroute(ctx, ["-U", "-O", "help", "127.0.0.1"])?
  assert udp.stdout == "No options for module `udp'\n"

  let icmp = traceroute(ctx, ["-I", "-O", "help", "127.0.0.1"])?
  assert "raw" in icmp.stdout and "dgram" in icmp.stdout

  let tcp = traceroute(ctx, ["-T", "-O", "help", "127.0.0.1"])?
  assert "syn" in tcp.stdout
}

test test_traceroute_help_and_version { |ctx|
  let help = traceroute(ctx, ["--help"])?
  assert help.status == 0
  assert help.stdout.starts_with("Usage: traceroute [OPTION]... HOST [PACKETLEN]\n"), help.stdout
  assert "--sendwait" in help.stdout

  let version = traceroute(ctx, ["--version"])?
  assert version.status == 0
  assert version.stdout.starts_with("traceroute ")

  let bad = traceroute(ctx, ["--no-such-option"])?
  assert bad.status == 2
  assert bad.stderr == "traceroute: unrecognized option '--no-such-option'\nTry 'traceroute --help' for more information.\n", bad.stderr
}
