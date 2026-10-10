# ping6 is the ping engine with the family pinned to IPv6; these tests run it
# over ::1 only.

type Ran = {status: Int, stdout: Str, stderr: Str}

proc ping6(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "ping6")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let entry = fp"{ctx.core_dir}/ping6.xsh"
  let argv = [ctx.xsh_bin.display(), entry.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""}, b"", out, err, timeout: 20s)
  let status = process.run(plan)?
  Ok({status: status.shell_code()?, stdout: out.read_text()?, stderr: err.read_text()?})
}

# Skips the test unless this user can send ICMPv6 echo over a datagram socket
# on ::1.
proc need_ipv6_echo() [process, error] -> Result[Unit] {
  let c = linux.net_constants()
  match linux.socket(c.AF_INET6, c.SOCK_DGRAM, c.IPPROTO_ICMPV6) {
    Err(failure) => {
      test.skip(f"ICMPv6 datagram sockets are not available ({failure.message})")
    }
    Ok(fd) => {
      unix.close_fd(fd)
    }
  }
  let probe = linux.socket(c.AF_INET6, c.SOCK_DGRAM)?
  defer unix.close_fd(probe)
  if let Err(failure) = linux.bind(probe, {family: "inet6", address: "::1", port: 0}) {
    test.skip(f"::1 is not configured here ({failure.message})")
  }
  Ok()
}

test test_ping6_reports_replies_and_statistics_over_loopback { |ctx|
  need_ipv6_echo()
  let ran = ping6(ctx, ["-c", "2", "-i", "0.2", "::1"])?
  assert ran.status == 0, ran.stderr
  assert ran.stderr == ""
  let lines = ran.stdout.split("\n")
  assert lines.len() == 8, ran.stdout
  assert lines[0] == "PING ::1 (::1) 56 data bytes"
  for index in [1, 2] {
    assert rx"^64 bytes from ::1: icmp_seq=[12] ttl=[0-9]+ time=[0-9]+\.[0-9]{1,3} ms$".matches(lines[index]), lines[index]
  }
  assert lines[3] == ""
  assert lines[4] == "--- ::1 ping statistics ---"
  assert rx"^2 packets transmitted, 2 received, 0% packet loss, time [0-9]+ms$".matches(lines[5]), lines[5]
  assert rx"^rtt min/avg/max/mdev = [0-9]+\.[0-9]{3}/[0-9]+\.[0-9]{3}/[0-9]+\.[0-9]{3}/[0-9]+\.[0-9]{3} ms$".matches(lines[6]), lines[6]
}

test test_ping6_header_names_the_source_when_an_interface_is_given { |ctx|
  need_ipv6_echo()
  let ran = ping6(ctx, ["-c", "1", "-I", "lo", "::1"])?
  assert ran.status == 0, ran.stderr
  assert ran.stdout.split("\n")[0] == "PING ::1 (::1) from ::1 lo: 56 data bytes"
}

test test_ping6_size_and_limits_follow_the_ipv6_header { |ctx|
  need_ipv6_echo()
  let sized = ping6(ctx, ["-c", "1", "-q", "-s", "200", "::1"])?
  assert sized.status == 0, sized.stderr
  assert sized.stdout.split("\n")[0] == "PING ::1 (::1) 200 data bytes"
  let large = ping6(ctx, ["-c", "1", "-s", "65528", "::1"])?
  assert large.status == 1
  assert large.stderr == "ping6: invalid -s value: '65528': out of range: 0 <= value <= 65527\n", large.stderr
}

test test_ping6_refuses_an_ipv4_destination_and_the_ipv4_flag { |ctx|
  let mismatch = ping6(ctx, ["-c", "1", "127.0.0.1"])?
  assert mismatch.status == 2
  assert mismatch.stderr == "ping6: 127.0.0.1: Address family for hostname not supported\n", mismatch.stderr
  let wrong = ping6(ctx, ["-4", "-c", "1", "::1"])?
  assert wrong.status == 2
  assert wrong.stderr == "ping6: only one -4 or -6 option may be specified\n", wrong.stderr
}

test test_ping6_hop_limit_option_is_applied_to_the_socket { |ctx|
  need_ipv6_echo()
  let ran = ping6(ctx, ["-c", "1", "-q", "-t", "7", "-Q", "0x10", "-M", "do", "::1"])?
  assert ran.status == 0, ran.stderr
  let rejected = ping6(ctx, ["-c", "1", "-t", "256", "::1"])?
  assert rejected.status == 1
  assert rejected.stderr == "ping6: invalid argument: '256': out of range: 0 <= value <= 255\n", rejected.stderr
}

test test_ping6_reports_a_failed_send_once_and_does_not_wait_for_a_reply { |ctx|
  need_ipv6_echo()?
  # With path MTU discovery set to "do", a datagram larger than the loopback
  # MTU is refused by the local stack on every send.
  let started = time.now()
  let ran = ping6(ctx, ["-c", "2", "-i", "0.2", "-M", "do", "-s", "65527", "::1"])?
  assert time.now() - started < 5000
  assert ran.status == 1
  assert rx"^ping6: sendmsg: Message too (large|long)\nping6: sendmsg: Message too (large|long)\n$".matches(ran.stderr), ran.stderr
  let lines = ran.stdout.split("\n")
  assert lines[0] == "PING ::1 (::1) 65527 data bytes"
  assert rx"^2 packets transmitted, 0 received, \+2 errors, 100% packet loss, time [0-9]+ms$".matches(lines[3]), ran.stdout
}
