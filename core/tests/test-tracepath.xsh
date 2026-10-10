# tracepath over loopback: the destination answers with ICMP port unreachable
# from the first hop. The layouts are the ones iputils prints; round-trip
# times vary and are compared by shape.

type Ran = {status: Int, stdout: Str, stderr: Str}

proc tracepath(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "tracepath")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let entry = fp"{ctx.core_dir}/tracepath.xsh"
  let argv = [ctx.xsh_bin.display(), entry.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""}, b"", out, err, timeout: 40s)
  let status = process.run(plan)?
  Ok({status: status.shell_code()?, stdout: out.read_text()?, stderr: err.read_text()?})
}

# Skips the test unless UDP over the loopback address of `family` can be
# bound, which the probes need.
proc need_loopback(family: Str) [process, error] -> Result[Unit] {
  let c = linux.net_constants()
  let domain = if family == "inet6" { c.AF_INET6 } else { c.AF_INET }
  let address = if family == "inet6" { "::1" } else { "127.0.0.1" }
  let probe = match linux.socket(domain, c.SOCK_DGRAM) {
    Ok(fd) => fd
    Err(failure) => {
      test.skip(f"UDP over {family} is not available ({failure.message})")
      return Ok()
    }
  }
  defer unix.close_fd(probe)
  if let Err(failure) = linux.bind(probe, {family: family, address: address, port: 0}) {
    test.skip(f"{address} is not configured here ({failure.message})")
  }
  Ok()
}

const HOP_V4 = rx"^ 1:  127\.0\.0\.1 +[0-9]+\.[0-9]{3}ms reached$"

test test_tracepath_reaches_the_loopback_destination_in_one_hop { |ctx|
  need_loopback("inet")
  let ran = tracepath(ctx, ["-n", "127.0.0.1"])?
  assert ran.status == 0, ran.stderr
  assert ran.stderr == ""
  let lines = ran.stdout.split("\n")
  assert lines.len() == 3, ran.stdout
  assert HOP_V4.matches(lines[0]), lines[0]
  # The host name occupies a fixed 52-column field, then the right-aligned time.
  assert lines[0].byte_len() == 74
  assert lines[1] == "     Resume: pmtu 65535 hops 1 back 1 "
  assert lines[2] == ""
}

test test_tracepath_packet_length_sets_the_path_mtu { |ctx|
  need_loopback("inet")
  let ran = tracepath(ctx, ["-n", "-l", "100", "127.0.0.1"])?
  assert ran.status == 0, ran.stderr
  assert ran.stdout.split("\n")[1] == "     Resume: pmtu 100 hops 1 back 1 "
}

test test_tracepath_names_the_hop_with_b_and_accepts_a_port { |ctx|
  need_loopback("inet")
  let both = tracepath(ctx, ["-b", "-p", "5000", "127.0.0.1"])?
  assert both.status == 0, both.stderr
  assert rx"^ 1:  [^ ]+ \(127\.0\.0\.1\) +[0-9]+\.[0-9]{3}ms reached$".matches(both.stdout.split("\n")[0]), both.stdout
  let slash = tracepath(ctx, ["-n", "127.0.0.1/5000"])?
  assert slash.status == 0, slash.stderr
  assert HOP_V4.matches(slash.stdout.split("\n")[0]), slash.stdout
}

test test_tracepath_stops_at_the_hop_limit { |ctx|
  need_loopback("inet")
  let ran = tracepath(ctx, ["-n", "-m", "0", "127.0.0.1"])?
  assert ran.status == 0, ran.stderr
  assert ran.stdout == "     Too many hops: pmtu 65535\n     Resume: pmtu 65535 \n", ran.stdout
}

test test_tracepath_discovers_the_ipv6_loopback_mtu { |ctx|
  need_loopback("inet6")
  let ran = tracepath(ctx, ["-n", "::1"])?
  assert ran.status == 0, ran.stderr
  let lines = ran.stdout.split("\n")
  # The first probe is larger than the interface MTU: the local stack reports
  # the MTU and the hop is probed again at that size.
  let reached = [line for line in lines if rx"^ 1:  ::1 +[0-9]+\.[0-9]{3}ms reached$".matches(line)]
  assert reached.len() >= 1, ran.stdout
  assert rx"^     Resume: pmtu [0-9]+ hops 1 back 1 $".matches(lines[lines.len() - 2]), ran.stdout
  for line in lines {
    if line.starts_with(" 1?:") {
      assert rx"^ 1\?: \[LOCALHOST\] +[0-9]+\.[0-9]{3}ms pmtu [0-9]+$".matches(line), line
    }
  }
}

test test_tracepath_reports_a_send_failure_to_a_broadcast_address { |ctx|
  need_loopback("inet")
  let ran = tracepath(ctx, ["-n", "127.255.255.255"])?
  assert ran.status == 0, ran.stderr
  assert ran.stdout == " 1:  send failed\n     Resume: pmtu 65535 \n", ran.stdout
}

test test_tracepath_rejects_bad_values_and_prints_usage { |ctx|
  for case in [
    {args: ["-l", "5", "127.0.0.1"], status: 1, stderr: "tracepath: pktlen must be within: 28 < value <= 2147483647\n"},
    {args: ["-p", "x", "127.0.0.1"], status: 1, stderr: "tracepath: invalid argument: 'x'\n"},
    {args: ["-m", "-1", "127.0.0.1"], status: 1, stderr: "tracepath: invalid argument: '-1': out of range: 0 <= value <= 255\n"},
    {args: ["-4", "-6", "127.0.0.1"], status: 1, stderr: "tracepath: Only one -4 or -6 option may be specified\n"},
    {args: ["nosuchhost.invalid"], status: 1, stderr: "tracepath: nosuchhost.invalid: Name or service not known\n"},
  ] {
    let ran = tracepath(ctx, case.args)?
    assert ran.status == case.status, f"{case.args.join(" ")}: status {ran.status}"
    assert ran.stderr == case.stderr, f"{case.args.join(" ")}: {ran.stderr}"
    assert ran.stdout == ""
  }
  for args in [[], ["-h"]] {
    let ran = tracepath(ctx, args)?
    assert ran.status == 255
    assert ran.stdout == ""
    assert ran.stderr.starts_with("\nUsage\n  tracepath [options] <destination>\n"), ran.stderr
  }
  let unknown = tracepath(ctx, ["-z", "127.0.0.1"])?
  assert unknown.status == 255
  assert unknown.stderr.starts_with("tracepath: invalid option -- 'z'\n"), unknown.stderr
  let version = tracepath(ctx, ["-V"])?
  assert version.status == 0
  assert rx"^tracepath \(XSH core\) ".matches(version.stdout), version.stdout
}
