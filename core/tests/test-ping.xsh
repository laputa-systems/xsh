use core.lib.icmp as icmp

# Every probe here goes to a loopback address. The output shapes below are
# the ones iputils prints for the same command lines on loopback; time values
# vary run to run, so they are compared by shape.

type Ran = {status: Int, stdout: Str, stderr: Str}

# Runs core/ping.xsh (or another applet of the same engine) by its real path so
# the invoked name and `lib.ping` resolve beside it.
proc ping_run(ctx: TestContext, script: Str, args: List[Str]) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "ping")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let entry = fp"{ctx.core_dir}/{script}"
  let argv = [ctx.xsh_bin.display(), entry.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""}, b"", out, err, timeout: 20s)
  let status = process.run(plan)?
  Ok({status: status.shell_code()?, stdout: out.read_text()?, stderr: err.read_text()?})
}

proc ping(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[Ran] {
  ping_run(ctx, "ping.xsh", args)
}

# Skips the test unless the kernel lets this user open an ICMP datagram socket
# of the family ("inet" or "inet6") and the loopback address is configured.
proc need_icmp(family: Str) [process, error] -> Result[Unit] {
  let c = linux.net_constants()
  let domain = if family == "inet6" { c.AF_INET6 } else { c.AF_INET }
  let protocol = if family == "inet6" { c.IPPROTO_ICMPV6 } else { c.IPPROTO_ICMP }
  match linux.socket(domain, c.SOCK_DGRAM, protocol) {
    Err(failure) => {
      test.skip(f"ICMP datagram sockets are not permitted for this group ({failure.message})")
    }
    Ok(fd) => {
      unix.close_fd(fd)
    }
  }
  let address = if family == "inet6" { "::1" } else { "127.0.0.1" }
  let probe = linux.socket(domain, c.SOCK_DGRAM)?
  defer unix.close_fd(probe)
  if let Err(failure) = linux.bind(probe, {family: family, address: address, port: 0}) {
    test.skip(f"{address} is not configured here ({failure.message})")
  }
  Ok()
}

const REPLY_V4 = rx"^64 bytes from 127\.0\.0\.1: icmp_seq=[0-9]+ ttl=[0-9]+ time=[0-9]+\.[0-9]{1,3} ms$"
const RTT_LINE = rx"^rtt min/avg/max/mdev = [0-9]+\.[0-9]{3}/[0-9]+\.[0-9]{3}/[0-9]+\.[0-9]{3}/[0-9]+\.[0-9]{3} ms$"

test test_icmp_checksum_and_echo_request_layout {
  # RFC 1071: the complement of the 16-bit one's-complement sum.
  assert icmp.checksum(b"\x08\x00\x00\x00\x00\x01\x00\x01") == 63485
  assert icmp.checksum(b"") == 65535
  assert icmp.checksum(b"\xff") == 255
  let payload = icmp.echo_stamped(icmp.echo_fill(20, b"")?, bytes.zero(16)?)
  let raw = icmp.echo_request("inet", 4660, 7, payload, true)?
  assert raw.slice(0, 2) == b"\x08\x00"
  assert bytes.unpack_be(raw, 2, 4)? == 4660
  assert bytes.unpack_be(raw, 2, 6)? == 7
  assert icmp.checksum_ok(raw)
  # The kernel fills in ICMPv6 checksums, and datagram sockets fill in both.
  let v6 = icmp.echo_request("inet6", 1, 2, b"", true)?
  assert v6 == b"\x80\x00\x00\x00\x00\x01\x00\x02"
  let datagram = icmp.echo_request("inet", 1, 2, b"", false)?
  assert datagram == b"\x08\x00\x00\x00\x00\x01\x00\x02"
  let echo = icmp.parse_echo(raw) ?? {kind: -1, code: -1, identifier: -1, sequence: -1, payload: b""}
  assert echo.kind == 8 and echo.identifier == 4660 and echo.sequence == 7
  assert echo.payload.len() == 20
  assert icmp.parse_echo(b"\x08\x00") == null
}

test test_icmp_payload_fill_and_pattern {
  # Without a pattern byte N holds N; the send time overwrites the front.
  let plain = icmp.echo_fill(20, b"")?
  assert plain.byte_at(0) == 0 and plain.byte_at(19) == 19
  let stamped = icmp.echo_stamped(plain, bytes.from_ints([255, 255, 255, 255])?)
  assert stamped.slice(0, 4) == b"\xff\xff\xff\xff"
  assert stamped.byte_at(4) == 4 and stamped.len() == 20
  assert icmp.echo_stamped(b"\x01", b"\xaa\xbb") == b"\x01"
  let patterned = icmp.echo_fill(5, b"\x12\x03")?
  assert patterned == b"\x12\x03\x12\x03\x12"
  # -p: hex digit pairs, an odd trailing digit stands alone, 16 bytes at most.
  assert icmp.pattern_bytes("ff") == b"\xff"
  assert icmp.pattern_bytes("123") == b"\x12\x03"
  assert icmp.pattern_bytes("zz") == null
  assert (icmp.pattern_bytes("00112233445566778899aabbccddeeff00") ?? b"") == b"\x00\x11\x22\x33\x44\x55\x66\x77\x88\x99\xaa\xbb\xcc\xdd\xee\xff"
}

test test_icmp_error_wording_and_address_text {
  assert icmp.icmp4_text(3, 1, 0) == "Destination Host Unreachable"
  assert icmp.icmp4_text(3, 4, 1400) == "Frag needed and DF set (mtu = 1400)"
  assert icmp.icmp4_text(11, 0, 0) == "Time to live exceeded"
  assert icmp.icmp4_text(3, 99, 0) == "Dest Unreachable, Bad Code: 99"
  assert icmp.icmp6_text(1, 3, 0) == "Destination unreachable: Address unreachable"
  assert icmp.icmp6_text(3, 0, 0) == "Time exceeded: Hop limit"
  assert icmp.icmp6_text(2, 0, 1280) == "Packet too big: mtu=1280"
  let loopback = bytes.concat([bytes.zero(15)?, b"\x01"])
  assert icmp.ipv6_text(loopback) == "::1"
  assert icmp.ipv6_text(bytes.concat([b"\x20\x01\x0d\xb8", bytes.zero(11)?, b"\x01"])) == "2001:db8::1"
  assert icmp.ipv6_text(bytes.zero(16)?) == "::"
  assert icmp.ipv6_text(bytes.concat([bytes.zero(10)?, b"\xff\xff\x7f\x00\x00\x01"])) == "::ffff:127.0.0.1"
  assert icmp.ipv4_text(b"\x7f\x00\x00\x01") == "127.0.0.1"
  # A queued error: errno 111, origin ICMP, type 3, code 3, then the offender.
  let extended = bytes.concat([bytes.pack_le(111, 4)?, b"\x02\x03\x03\x00", bytes.zero(8)?, b"\x02\x00\x00\x00\x7f\x00\x00\x01", bytes.zero(8)?])
  let queued = icmp.parse_queued_error(extended, b"x") ?? {errno: 0, origin: 0, kind: 0, code: 0, info: 0, offender: "", payload: b"", port: 0, ttl: null, stamp_ns: null}
  assert queued.errno == 111 and queued.origin == 2 and queued.kind == 3 and queued.code == 3
  assert queued.offender == "127.0.0.1"
}

test test_ping_reports_replies_and_statistics_over_ipv4 { |ctx|
  need_icmp("inet")
  let ran = ping(ctx, ["-c", "2", "-i", "0.2", "127.0.0.1"])?
  assert ran.status == 0, ran.stderr
  assert ran.stderr == ""
  let lines = ran.stdout.split("\n")
  assert lines.len() == 8, ran.stdout
  assert lines[0] == "PING 127.0.0.1 (127.0.0.1) 56(84) bytes of data."
  assert REPLY_V4.matches(lines[1]), lines[1]
  assert REPLY_V4.matches(lines[2]), lines[2]
  assert "icmp_seq=1 " in lines[1] and "icmp_seq=2 " in lines[2]
  assert lines[3] == ""
  assert lines[4] == "--- 127.0.0.1 ping statistics ---"
  assert rx"^2 packets transmitted, 2 received, 0% packet loss, time [0-9]+ms$".matches(lines[5]), lines[5]
  assert RTT_LINE.matches(lines[6]), lines[6]
  assert lines[7] == ""
}

test test_ping_reports_replies_over_ipv6 { |ctx|
  need_icmp("inet6")
  let ran = ping(ctx, ["-c", "1", "::1"])?
  assert ran.status == 0, ran.stderr
  let lines = ran.stdout.split("\n")
  assert lines[0] == "PING ::1 (::1) 56 data bytes"
  assert rx"^64 bytes from ::1: icmp_seq=1 ttl=[0-9]+ time=[0-9]+\.[0-9]{1,3} ms$".matches(lines[1]), lines[1]
  assert lines[3] == "--- ::1 ping statistics ---"
  assert lines[4] == "1 packets transmitted, 1 received, 0% packet loss, time 0ms" or rx"^1 packets transmitted, 1 received, 0% packet loss, time [01]ms$".matches(lines[4]), lines[4]
}

test test_ping_size_quiet_timestamps_and_pattern { |ctx|
  need_icmp("inet")
  let sized = ping(ctx, ["-c", "1", "-s", "100", "127.0.0.1"])?
  assert sized.status == 0, sized.stderr
  let sized_lines = sized.stdout.split("\n")
  assert sized_lines[0] == "PING 127.0.0.1 (127.0.0.1) 100(128) bytes of data."
  assert rx"^108 bytes from 127\.0\.0\.1: icmp_seq=1 ".matches(sized_lines[1]), sized_lines[1]

  let quiet = ping(ctx, ["-c", "1", "-q", "127.0.0.1"])?
  assert quiet.status == 0
  let quiet_lines = quiet.stdout.split("\n")
  assert quiet_lines[0] == "PING 127.0.0.1 (127.0.0.1) 56(84) bytes of data."
  assert quiet_lines[1] == ""
  assert quiet_lines[2] == "--- 127.0.0.1 ping statistics ---"

  let stamped = ping(ctx, ["-c", "1", "-D", "127.0.0.1"])?
  assert stamped.status == 0
  assert rx"^\[[0-9]+\.[0-9]{6}\] 64 bytes from 127\.0\.0\.1: icmp_seq=1 ".matches(stamped.stdout.split("\n")[1]), stamped.stdout

  let pattern = ping(ctx, ["-c", "1", "-p", "123", "127.0.0.1"])?
  assert pattern.status == 0
  assert pattern.stdout.split("\n")[0] == "PATTERN: 0x1203"
  assert pattern.stdout.split("\n")[1] == "PING 127.0.0.1 (127.0.0.1) 56(84) bytes of data."

  let big = ping(ctx, ["-c", "1", "-s", "65507", "127.0.0.1"])?
  assert big.status == 0, big.stderr
  assert big.stdout.split("\n")[0] == "PING 127.0.0.1 (127.0.0.1) 65507(65535) bytes of data."
}

test test_ping_source_interface_and_address_in_the_header { |ctx|
  need_icmp("inet")
  let by_name = ping(ctx, ["-c", "1", "-I", "lo", "127.0.0.1"])?
  assert by_name.status == 0, by_name.stderr
  assert by_name.stdout.split("\n")[0] == "PING 127.0.0.1 (127.0.0.1) from 127.0.0.1 lo: 56(84) bytes of data."
  let by_address = ping(ctx, ["-c", "1", "-I", "127.0.0.1", "127.0.0.1"])?
  assert by_address.status == 0, by_address.stderr
  assert by_address.stdout.split("\n")[0] == "PING 127.0.0.1 (127.0.0.1) from 127.0.0.1 : 56(84) bytes of data."
  let sticky = ping(ctx, ["-c", "1", "-B", "127.0.0.1"])?
  assert sticky.status == 0, sticky.stderr
  assert sticky.stdout.split("\n")[0] == "PING 127.0.0.1 (127.0.0.1) from 127.0.0.1 : 56(84) bytes of data."
  let missing = ping(ctx, ["-c", "1", "-I", "nosuch0", "127.0.0.1"])?
  assert missing.status == 2
  assert missing.stderr == "ping: SO_BINDTODEVICE nosuch0: No such device\n", missing.stderr
  let foreign = ping(ctx, ["-c", "1", "-I", "10.9.9.9", "127.0.0.1"])?
  assert foreign.status == 2
  assert foreign.stderr == "ping: bind: Address not available\n", foreign.stderr
}

test test_ping_names_follow_numeric_and_resolve_options { |ctx|
  need_icmp("inet")
  if let Err(_) = dns.resolve_host("localhost", "ipv4") {
    test.skip("localhost does not resolve here")
  }
  let numeric = ping(ctx, ["-4", "-n", "-c", "1", "localhost"])?
  assert numeric.status == 0, numeric.stderr
  let lines = numeric.stdout.split("\n")
  assert lines[0] == "PING localhost (127.0.0.1) 56(84) bytes of data."
  assert rx"^64 bytes from 127\.0\.0\.1: icmp_seq=1 ".matches(lines[1]), lines[1]
  assert lines[3] == "--- localhost ping statistics ---"

  let named = ping(ctx, ["-4", "-c", "1", "localhost"])?
  assert named.status == 0, named.stderr
  assert rx"^64 bytes from [^ ]+( \(127\.0\.0\.1\))?: icmp_seq=1 ".matches(named.stdout.split("\n")[1]), named.stdout

  let forced = ping(ctx, ["-H", "-c", "1", "127.0.0.1"])?
  assert forced.status == 0, forced.stderr
  assert rx"^64 bytes from ([^ ]+ \(127\.0\.0\.1\)|127\.0\.0\.1): icmp_seq=1 ".matches(forced.stdout.split("\n")[1]), forced.stdout
}

test test_ping_deadline_with_a_count_it_cannot_reach_exits_1 { |ctx|
  need_icmp("inet")
  let ran = ping(ctx, ["-c", "5", "-w", "1", "-i", "0.4", "127.0.0.1"])?
  assert ran.status == 1, ran.stderr
  assert rx"\n3 packets transmitted, 3 received, 0% packet loss, time [0-9]+ms\n".matches(ran.stdout), ran.stdout
}

test test_ping_preload_adaptive_and_outstanding_options_report_what_they_did { |ctx|
  need_icmp("inet")
  let preload = ping(ctx, ["-c", "3", "-l", "3", "-q", "127.0.0.1"])?
  assert preload.status == 0, preload.stderr
  assert rx"3 packets transmitted, 3 received, 0% packet loss, time [0-9]+ms\nrtt min/avg/max/mdev = [0-9.\/]+ ms, pipe 3\n".matches(preload.stdout), preload.stdout

  let adaptive = ping(ctx, ["-c", "3", "-A", "-q", "127.0.0.1"])?
  assert adaptive.status == 0, adaptive.stderr
  assert rx"ms, ipg/ewma [0-9]+\.[0-9]{3}/[0-9]+\.[0-9]{3} ms\n$".matches(adaptive.stdout), adaptive.stdout

  let outstanding = ping(ctx, ["-c", "1", "-O", "127.0.0.1"])?
  assert outstanding.status == 0
  assert "no answer yet" not in outstanding.stdout

  let audible = ping(ctx, ["-c", "1", "-a", "127.0.0.1"])?
  assert audible.status == 0
  assert "\u{7}64 bytes from" in audible.stdout
}

test test_ping_socket_options_are_applied_or_refused_by_the_kernel { |ctx|
  need_icmp("inet")
  for hint in ["do", "dont", "want", "probe"] {
    let ran = ping(ctx, ["-c", "1", "-q", "-M", hint, "127.0.0.1"])?
    assert ran.status == 0, f"{hint}: {ran.stderr}"
  }
  for args in [["-t", "5"], ["-Q", "0x10"], ["-Q", "16"], ["-S", "65536"], ["-r"], ["-L"], ["-v"], ["-b"], ["-n"], ["-W", "0.5"], ["-i", "0.5"]] {
    let ran = ping(ctx, ["-c", "1", "-q"].extend(args).extend(["127.0.0.1"]))?
    assert ran.status == 0, f"{args.join(" ")}: {ran.stderr}"
  }
  # A mark and the debug flag need CAP_NET_ADMIN: the kernel's answer is
  # reported as it is, never ignored.
  for option in ["-m", "-d"] {
    let args = if option == "-m" { ["-c", "1", "-q", "-m", "5", "127.0.0.1"] } else { ["-c", "1", "-q", "-d", "127.0.0.1"] }
    let ran = ping(ctx, args)?
    if ran.status != 0 {
      assert ran.status == 2
      assert rx"^ping: SO_(MARK|DEBUG): (Operation not permitted|Permission denied)\n$".matches(ran.stderr), ran.stderr
    }
  }
  let verbose = ping(ctx, ["-c", "1", "-v", "127.0.0.1"])?
  assert rx"^ping: sock4\.fd: [0-9]+ \(socktype: SOCK_(DGRAM|RAW)\), hints\.ai_family: AF_INET\nping: ai->ai_family: AF_INET, ai->ai_canonname: '127\.0\.0\.1'\n$".matches(verbose.stderr), verbose.stderr
}

test test_ping_broadcast_needs_the_b_option { |ctx|
  need_icmp("inet")
  let refused = ping(ctx, ["-c", "1", "127.255.255.255"])?
  assert refused.status == 2
  assert refused.stdout == ""
  assert refused.stderr == "ping: Do you want to ping broadcast? Then -b. If not, check your local firewall rules\n", refused.stderr
  let allowed = ping(ctx, ["-c", "1", "-b", "-W", "0.3", "127.255.255.255"])?
  assert allowed.stdout.starts_with("PING 127.255.255.255 (127.255.255.255) 56(84) bytes of data.\nWARNING: pinging broadcast address\n"), allowed.stdout
  assert allowed.status == 0 or allowed.status == 1
}

test test_ping_rejects_bad_values_with_the_iputils_wording { |ctx|
  for case in [
    {args: ["-c", "0", "127.0.0.1"], status: 1, stderr: "ping: invalid argument: '0': out of range: 1 <= value <= 9223372036854775807\n"},
    {args: ["-c", "x", "127.0.0.1"], status: 1, stderr: "ping: invalid argument: 'x'\n"},
    {args: ["-c", "1x", "127.0.0.1"], status: 1, stderr: "ping: invalid argument: '1x'\n"},
    {args: ["-s", "99999", "127.0.0.1"], status: 1, stderr: "ping: invalid -s value: '99999': out of range: 0 <= value <= 65507\n"},
    {args: ["-s", "-1", "127.0.0.1"], status: 1, stderr: "ping: invalid argument: '-1': out of range: 0 <= value <= 2147483647\n"},
    {args: ["-t", "300", "127.0.0.1"], status: 1, stderr: "ping: invalid argument: '300': out of range: 0 <= value <= 255\n"},
    {args: ["-w", "x", "127.0.0.1"], status: 1, stderr: "ping: invalid argument: 'x'\n"},
    {args: ["-W", "-1", "-c", "1", "127.0.0.1"], status: 2, stderr: "ping: bad linger time: -1\n"},
    {args: ["-p", "zz", "127.0.0.1"], status: 2, stderr: "ping: patterns must be specified as hex digits: zz\n"},
    {args: ["-M", "foo", "127.0.0.1"], status: 2, stderr: "ping: invalid -M argument: foo\n"},
    {args: ["-Q", "300", "127.0.0.1"], status: 2, stderr: "ping: the decimal value of TOS bits must be in range 0-255: 300\n"},
    {args: ["-4", "-6", "127.0.0.1"], status: 2, stderr: "ping: only one -4 or -6 option may be specified\n"},
    {args: ["-4", "::1"], status: 2, stderr: "ping: ::1: Address family for hostname not supported\n"},
    {args: [], status: 2, stderr: "ping: usage error: Destination address required\n"},
    {args: ["-c", "1", "nosuchhost.invalid"], status: 2, stderr: "ping: nosuchhost.invalid: Name or service not known\n"},
  ] {
    let ran = ping(ctx, case.args)?
    assert ran.status == case.status, f"{case.args.join(" ")}: status {ran.status}"
    assert ran.stderr == case.stderr, f"{case.args.join(" ")}: {ran.stderr}"
    assert ran.stdout == "", f"{case.args.join(" ")}: {ran.stdout}"
  }
  let ttl = ping(ctx, ["-t", "0", "-c", "1", "127.0.0.1"])?
  assert ttl.status == 2
  assert ttl.stderr == "ping: cannot set unicast time-to-live: Invalid argument\n", ttl.stderr
}

test test_ping_limits_the_interval_and_preload_of_unprivileged_users { |ctx|
  if unix.id()?.uid == 0 {
    test.skip("the interval and preload limits do not apply to root")
  }
  let flood = ping(ctx, ["-i", "0.001", "-c", "1", "127.0.0.1"])?
  assert flood.status == 2
  assert flood.stderr == "ping: cannot flood, minimal interval for user must be >= 2 ms, use -i 0.002 (or higher)\n", flood.stderr
  # Text after the number is tolerated with a warning; "abc" reads as zero.
  let garbage = ping(ctx, ["-i", "abc", "-c", "1", "127.0.0.1"])?
  assert garbage.status == 2
  assert garbage.stderr.starts_with("ping: option argument contains garbage: abc\nping: this will become fatal error in the future\n"), garbage.stderr
  let preload = ping(ctx, ["-l", "5", "-c", "1", "127.0.0.1"])?
  assert preload.status == 2
  assert preload.stderr == "ping: cannot set preload to value greater than 3: 5\n", preload.stderr
}

test test_ping_refuses_options_it_cannot_honor { |ctx|
  for option in ["-f", "-3", "-C", "-U", "-R", "-T", "-N", "-F"] {
    let ran = ping(ctx, [option, "127.0.0.1"])?
    assert ran.status == 2, option
    assert rx"^ping: option '-.' is not supported: ".matches(ran.stderr), ran.stderr
    assert ran.stdout == "", option
  }
  let unknown = ping(ctx, ["-z", "127.0.0.1"])?
  assert unknown.status == 2
  assert unknown.stderr.starts_with("ping: invalid option -- 'z'\n"), unknown.stderr
}

test test_ping_help_and_version { |ctx|
  let help = ping(ctx, ["-h"])?
  assert help.status == 2
  assert help.stdout == ""
  assert help.stderr.starts_with("\nUsage\n  ping [options] <destination>"), help.stderr
  let version = ping(ctx, ["-V"])?
  assert version.status == 0
  assert rx"^ping \(XSH core\) ".matches(version.stdout), version.stdout
}

test test_ping_raw_socket_identifier_needs_cap_net_raw { |ctx|
  need_icmp("inet")
  let c = linux.net_constants()
  let ran = ping(ctx, ["-c", "1", "-e", "77", "127.0.0.1"])?
  match linux.socket(c.AF_INET, c.SOCK_RAW, c.IPPROTO_ICMP) {
    Ok(fd) => {
      unix.close_fd(fd)
      assert ran.status == 0, ran.stderr
      assert REPLY_V4.matches(ran.stdout.split("\n")[1]), ran.stdout
    }
    Err(_) => {
      assert ran.status == 2
      assert ran.stderr == "ping: socktype: SOCK_RAW\nping: socket: Operation not permitted\nping: => missing cap_net_raw+p capability or setuid?\n", ran.stderr
    }
  }
}

test test_ping_interrupt_prints_the_statistics_and_the_reply_status { |ctx|
  need_icmp("inet")
  let root = test.temp_dir(ctx, name: "ping-interrupt")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let entry = fp"{ctx.core_dir}/ping.xsh"
  let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), entry.display(), "-i", "0.2", "127.0.0.1"], root, {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""}, b"", out, err)
  let handle = spawn plan?
  let started = time.now()
  while "icmp_seq=2 " not in out.read_text()? {
    if time.now() - started > 5000 {
      handle.cancel(signal: "KILL", kill_after: 0ms)
      return Err(error.failure("ping printed no second reply"))
    }
    time.sleep(50ms)
  }
  process.kill(handle.pid, "INT")
  let completed = process.wait_timeout([handle], 5s)?
  guard let finished = completed else {
    handle.cancel(signal: "KILL", kill_after: 0ms)
    return Err(error.failure("ping did not stop after SIGINT"))
  }
  assert finished.status.shell_code()? == 0, err.read_text()?
  let text = out.read_text()?
  assert "\n--- 127.0.0.1 ping statistics ---\n" in text, text
  assert rx"\n[0-9]+ packets transmitted, [0-9]+ received, 0% packet loss, time [0-9]+ms\nrtt min/avg/max/mdev = ".matches(text), text
}
