# Transcripts here were pinned against netcat-openbsd (Debian patchlevel
# 1.234-1) on loopback. Every socket stays on 127.0.0.1 or ::1 or a path in
# the test's own temporary directory; the peer of each `nc` under test is
# either another `nc` or a socket this file opens itself.

type Ran = {status: Int, stdout: Bytes, stderr: Str}
type Listener = {fd: Int, port: Int}

const USAGE = "usage: nc [-46CDdFhklNnrStUuvZz] [-I length] [-i interval] [-M ttl]\n\t  [-m minttl] [-O length] [-P proxy_username] [-p source_port]\n\t  [-q seconds] [-s sourceaddr] [-T keyword] [-V rtable] [-W recvlimit]\n\t  [-w timeout] [-X proxy_protocol] [-x proxy_address[:port]]\n\t  [destination] [port]\n"

proc nc_command(ctx: TestContext, root: Path, tag: Str, args: List[Str], stdin: Bytes = b"") [process] -> Command {
  let script = fp"{ctx.core_dir}/nc.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  process.command_argv(
    ctx.xsh_bin,
    argv,
    root,
    {LC_ALL: "C"},
    stdin,
    fp"{root}/{tag}.out",
    fp"{root}/{tag}.err",
    timeout: 30s,
  )
}

proc finished(root: Path, tag: Str, status: Status) [fs, process, error] -> Result[Ran] {
  Ok({status: status.shell_code()?, stdout: fp"{root}/{tag}.out".read_bytes()?, stderr: fp"{root}/{tag}.err".read_text()?})
}

proc run_nc(ctx: TestContext, args: List[Str], stdin: Bytes = b"") [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "nc")?
  let status = process.run(nc_command(ctx, root, "run", args, stdin))?
  finished(root, "run", status)
}

# Polls a spawned process's stderr until it shows `needle`, which is how a
# listener says it is ready without a probe connection stealing its accept.
proc await_text(file: Path, needle: Str) [fs, time, error] -> Result[Unit] {
  let started = time.now()
  while time.now() - started < 10000 {
    if (file.read_text() ?? "").find(needle) != null {
      return Ok()
    }
    time.sleep(20ms)?
  }
  Err(error.failure(f"{file} never showed {needle}"))
}

proc tcp_listener(address: Str = "127.0.0.1") [net, process, error] -> Result[Listener] {
  let c = linux.net_constants()
  let family = if address.find(":") != null { c.AF_INET6 } else { c.AF_INET }
  let fd = linux.socket(family, c.SOCK_STREAM)?
  linux.bind(fd, {family: if address.find(":") != null { "inet6" } else { "inet" }, address: address, port: 0})?
  linux.listen(fd, 8)?
  Ok({fd: fd, port: linux.getsockname(fd)?.port})
}

# A port nothing listens on at the moment the listener closes.
proc closed_port() [net, process, error] -> Result[Int] {
  let held = tcp_listener()?
  unix.close_fd(held.fd)?
  Ok(held.port)
}

proc wait_readable(fd: Int) [net, process, error] -> Result[Unit] {
  let events = unix.poll_fd(fd, ["readable"], 10000)?
  if events.is_empty() {
    return Err(error.failure("nothing arrived within 10 seconds"))
  }
  Ok()
}

proc read_to_eof(fd: Int) [net, process, error] -> Result[Bytes] {
  var chunks: List[Bytes] = []
  while true {
    wait_readable(fd)?
    let data = unix.read_fd(fd, 65536)?
    if data.is_empty() {
      break
    }
    chunks += [data]
  }
  Ok(bytes.concat(chunks))
}

proc send_all(fd: Int, data: Bytes) [net, process, error] -> Result[Unit] {
  var rest = data
  while ! rest.is_empty() {
    rest = rest.slice(unix.write_fd(fd, rest)?)
  }
  Ok()
}

# Opens a TCP connection to `port` on loopback from the test itself.
proc tcp_client(port: Int, address: Str = "127.0.0.1") [net, process, error] -> Result[Int] {
  let c = linux.net_constants()
  let v6 = address.find(":") != null
  let fd = linux.socket(if v6 { c.AF_INET6 } else { c.AF_INET }, c.SOCK_STREAM)?
  linux.connect(fd, {family: if v6 { "inet6" } else { "inet" }, address: address, port: port})?
  Ok(fd)
}

proc udp_socket() [net, process, error] -> Result[Listener] {
  let c = linux.net_constants()
  let fd = linux.socket(c.AF_INET, c.SOCK_DGRAM)?
  linux.bind(fd, {family: "inet", address: "127.0.0.1", port: 0})?
  Ok({fd: fd, port: linux.getsockname(fd)?.port})
}

# A payload that covers every byte value and spans several relay buffers.
proc pattern(length: Int) [error] -> Result[Bytes] {
  var chunks: List[Bytes] = []
  var made = 0
  let block = bytes.from_ints([n for n in range(256)])?
  while made < length {
    chunks += [block]
    made += 256
  }
  Ok(bytes.concat(chunks).slice(0, length))
}

test test_nc_help_and_usage_match_the_utility { |ctx|
  let help = run_nc(ctx, ["-h"])?
  assert help.status == 0
  assert help.stdout == b""
  assert help.stderr.starts_with("OpenBSD netcat (Debian patchlevel 1.234-1)\n" + USAGE + "\tCommand Summary:\n"), help.stderr
  assert help.stderr.ends_with("\tPort numbers can be individual or ranges: lo-hi [inclusive]\n")

  let bare = run_nc(ctx, [])?
  assert bare.status == 1
  assert bare.stderr == USAGE, bare.stderr

  let unknown = run_nc(ctx, ["-Y"])?
  assert unknown.status == 1
  assert unknown.stderr == "nc: unrecognized option: Y\n" + USAGE, unknown.stderr

  let missing = run_nc(ctx, ["-x"])?
  assert missing.status == 1
  assert missing.stderr == "nc: option requires an argument: x\n" + USAGE, missing.stderr
}

test test_nc_diagnoses_bad_operands_and_values { |ctx|
  let cases: List[List[Str]] = [
    ["-l"], ["127.0.0.1"], ["-lk", "-z", "5700"], ["-zk", "127.0.0.1", "5700"],
    ["-lv", "1", "2", "3"], ["-l", "-s", "127.0.0.1", "5700"], ["-l", "-p", "5700", "5701"],
    ["-U"], ["-U", "/a", "/b"], ["-U", "-p", "5", "/a"], ["-U", "-F", "/a"],
    ["127.0.0.1", "70000"], ["127.0.0.1", "0"], ["127.0.0.1", "1-2-3"], ["127.0.0.1", "1-yy"],
    ["127.0.0.1", "xx-3"], ["127.0.0.1", "0x5"], ["127.0.0.1", "5612,5610"], ["127.0.0.1", "abc"],
    ["127.0.0.1", "-5"], ["127.0.0.1", "1-65536"], ["127.0.0.1", ""],
    ["-w", "abc", "127.0.0.1", "5"], ["-w", "-1", "127.0.0.1", "5"], ["-w", "2147484", "127.0.0.1", "5"],
    ["-i", "abc", "127.0.0.1", "5"], ["-i", "-1", "127.0.0.1", "5"], ["-q", "abc", "127.0.0.1", "5"],
    ["-W", "abc", "-l", "5"], ["-W", "0", "-l", "5"], ["-I", "abc", "127.0.0.1", "5"], ["-I", "-1", "127.0.0.1", "5"],
    ["-M", "abc", "127.0.0.1", "5"], ["-M", "256", "127.0.0.1", "5"], ["-m", "256", "127.0.0.1", "5"],
    ["-T", "bogus", "127.0.0.1", "5"], ["-T", "300", "127.0.0.1", "5"], ["-X", "bogus", "127.0.0.1", "5"],
    ["-V", "1", "127.0.0.1", "5"], ["-4", "::1", "5"], ["-6", "127.0.0.1", "5"], ["-n", "localhost", "5"],
  ]
  let expected = [
    "nc: missing port number\n", "nc: missing port number\n", "nc: cannot use -z and -l\n", "nc: must use -l with -k\n",
    USAGE, USAGE, USAGE,
    USAGE, "nc: cannot use port with -U\n", USAGE, "nc: cannot use -F and -U\n",
    "nc: port number too large: 70000\n", "nc: port number too small: 0\n", "nc: port number invalid: 2-3\n", "nc: port number invalid: yy\n",
    "nc: service \"xx-3\" unknown\n", "nc: port number invalid: 0x5\n", "nc: port number invalid: 5612,5610\n", "nc: service \"abc\" unknown\n",
    "nc: port number too small: -5\n", "nc: port number too large: 65536\n", "nc: service \"\" unknown\n",
    "nc: timeout invalid: abc\n", "nc: timeout too small: -1\n", "nc: timeout too large: 2147484\n",
    "nc: interval invalid: abc\n", "nc: interval too small: -1\n", "nc: quit timer invalid: abc\n",
    "nc: receive limit invalid: abc\n", "nc: receive limit too small: 0\n", "nc: TCP receive window invalid: abc\n", "nc: TCP receive window too small: -1\n",
    "nc: ttl is invalid\n", "nc: ttl is too large\n", "nc: minttl is too large\n",
    "nc: illegal tos value bogus\n", "nc: illegal tos value 300\n", "nc: unsupported proxy protocol\n",
    "nc: no alternate routing table support available\n",
    "nc: getaddrinfo for host \"::1\" port 5: Name has no usable address\n",
    "nc: getaddrinfo for host \"127.0.0.1\" port 5: Name has no usable address\n",
    "nc: getaddrinfo for host \"localhost\" port 5: Name does not resolve\n",
  ]
  assert cases.len() == expected.len()
  for index in range(cases.len()) {
    let ran = run_nc(ctx, cases[index])?
    assert ran.status == 1, f"{cases[index].join(" ")} exited {ran.status}"
    assert ran.stderr == expected[index], f"{cases[index].join(" ")}: {ran.stderr}"
  }
}

# The options below change no behavior on Linux the utility could honor, so
# each names why it stops instead of being accepted and ignored.
test test_nc_refuses_unimplemented_options_by_name { |ctx|
  let cases: List[List[Str]] = [
    ["-x", "127.0.0.1:9", "127.0.0.1", "22"], ["-X", "5", "127.0.0.1", "22"], ["-P", "user", "127.0.0.1", "22"],
    ["-S", "127.0.0.1", "22"], ["-F", "127.0.0.1", "22"], ["-Z", "127.0.0.1", "22"],
  ]
  let expected = [
    "nc: proxy connections (-X, -x, -P) are not supported\n", "nc: proxy connections (-X, -x, -P) are not supported\n",
    "nc: proxy connections (-X, -x, -P) are not supported\n",
    "nc: -S (TCP MD5 signatures) is not supported\n", "nc: -F (file descriptor passing) is not supported\n",
    "nc: -Z (DCCP) is not supported\n",
  ]
  for index in range(cases.len()) {
    let ran = run_nc(ctx, cases[index])?
    assert ran.status == 1, f"{cases[index].join(" ")} exited {ran.status}"
    assert ran.stdout == b""
    assert ran.stderr == expected[index], f"{cases[index].join(" ")}: {ran.stderr}"
  }
}

test test_nc_client_sends_stdin_and_prints_the_reply { |ctx|
  let server = tcp_listener()?
  defer unix.close_fd(server.fd)
  let root = test.temp_dir(ctx, name: "nc-client")?
  let payload = pattern(200000)?
  let handle = spawn nc_command(ctx, root, "client", ["-v", "-n", "-N", "127.0.0.1", f"{server.port}"], payload)?

  wait_readable(server.fd)?
  let accepted = linux.accept(server.fd)?
  assert read_to_eof(accepted.fd)? == payload
  send_all(accepted.fd, b"reply\n")?
  unix.close_fd(accepted.fd)?

  let status = wait handle?
  let ran = finished(root, "client", status)?
  assert ran.status == 0
  assert ran.stdout == b"reply\n"
  assert ran.stderr == f"Connection to 127.0.0.1 {server.port} port [tcp/*] succeeded!\n", ran.stderr
}

test test_nc_listener_receives_binary_stdin_and_replies { |ctx|
  let port = closed_port()?
  let root = test.temp_dir(ctx, name: "nc-listener")?
  let handle = spawn nc_command(ctx, root, "server", ["-v", "-n", "-l", "127.0.0.1", f"{port}"], b"srv-reply\n")?
  await_text(fp"{root}/server.err", "Listening on")?

  let client = tcp_client(port)?
  defer unix.close_fd(client)
  let local = linux.getsockname(client)?
  let payload = pattern(70000)?
  send_all(client, payload)?
  linux.shutdown(client, linux.net_constants().SHUT_WR)?
  assert read_to_eof(client)? == b"srv-reply\n"

  let status = wait handle?
  let ran = finished(root, "server", status)?
  assert ran.status == 0
  assert ran.stdout == payload
  assert ran.stderr == f"Listening on 127.0.0.1 {port}\nConnection received on 127.0.0.1 {local.port}\n", ran.stderr
}

test test_nc_talks_to_itself_and_half_closes { |ctx|
  let port = closed_port()?
  let root = test.temp_dir(ctx, name: "nc-pair")?
  let server = spawn nc_command(ctx, root, "server", ["-v", "-n", "-l", "127.0.0.1", f"{port}"], b"from the listener\n")?
  await_text(fp"{root}/server.err", "Listening on")?

  let client = run_nc(ctx, ["-v", "-n", "-N", "127.0.0.1", f"{port}"], b"from the client\n")?
  assert client.status == 0
  assert client.stdout == b"from the listener\n"
  assert client.stderr == f"Connection to 127.0.0.1 {port} port [tcp/*] succeeded!\n", client.stderr

  let served = finished(root, "server", wait server?)?
  assert served.status == 0
  assert served.stdout == b"from the client\n"
}

test test_nc_listen_without_n_names_the_address { |ctx|
  match dns.reverse("127.0.0.1") {
    Ok(names) => {
      if names != ["localhost"] {
        test.skip("127.0.0.1 does not reverse-resolve to localhost here")
        return
      }
    }
    Err(_) => {
      test.skip("127.0.0.1 does not reverse-resolve here")
      return
    }
  }
  let port = closed_port()?
  let root = test.temp_dir(ctx, name: "nc-names")?
  let handle = spawn nc_command(ctx, root, "server", ["-v", "-l", "127.0.0.1", f"{port}"])?
  await_text(fp"{root}/server.err", "Listening on")?
  let client = tcp_client(port)?
  let local = linux.getsockname(client)?
  unix.close_fd(client)?
  let ran = finished(root, "server", wait handle?)?
  assert ran.stderr == f"Listening on localhost {port}\nConnection received on localhost {local.port}\n", ran.stderr
}

test test_nc_without_n_to_a_name_prints_the_address_it_used { |ctx|
  match dns.resolve_host("localhost") {
    Ok(rows) => {
      if [r for r in rows if r.addr == "127.0.0.1"].is_empty() {
        test.skip("localhost has no IPv4 address here")
        return
      }
    }
    Err(_) => {
      test.skip("localhost does not resolve here")
      return
    }
  }
  let server = tcp_listener()?
  defer unix.close_fd(server.fd)
  let ran = run_nc(ctx, ["-4", "-z", "-v", "localhost", f"{server.port}"])?
  assert ran.status == 0
  assert ran.stderr == f"Connection to localhost (127.0.0.1) {server.port} port [tcp/*] succeeded!\n", ran.stderr
}

test test_nc_scan_reports_open_and_closed_ports_in_order { |ctx|
  let open_a = tcp_listener()?
  defer unix.close_fd(open_a.fd)
  let open_b = tcp_listener()?
  defer unix.close_fd(open_b.fd)
  let shut = closed_port()?

  let verbose = run_nc(ctx, ["-vz", "-n", "127.0.0.1", f"{open_b.port}", f"{shut}", f"{open_a.port}"])?
  assert verbose.status == 0
  assert verbose.stdout == b""
  let want = f"Connection to 127.0.0.1 {open_b.port} port [tcp/*] succeeded!\nnc: connect to 127.0.0.1 port {shut} (tcp) failed: Connection refused\nConnection to 127.0.0.1 {open_a.port} port [tcp/*] succeeded!\n"
  assert verbose.stderr == want, verbose.stderr

  # -z alone reports successes and stays quiet about refusals.
  let terse = run_nc(ctx, ["-z", "-n", "127.0.0.1", f"{shut}", f"{open_a.port}"])?
  assert terse.status == 0
  assert terse.stderr == f"Connection to 127.0.0.1 {open_a.port} port [tcp/*] succeeded!\n", terse.stderr

  let refused = run_nc(ctx, ["-z", "-n", "127.0.0.1", f"{shut}"])?
  assert refused.status == 1
  assert refused.stderr == ""
  let loud = run_nc(ctx, ["-vz", "-n", "127.0.0.1", f"{shut}"])?
  assert loud.status == 1
  assert loud.stderr == f"nc: connect to 127.0.0.1 port {shut} (tcp) failed: Connection refused\n", loud.stderr
}

test test_nc_scans_ascending_ranges_even_when_given_backwards { |ctx|
  # Three consecutive ports of which only the middle one listens.
  var base = 0
  var middle = -1
  for _ in range(20) {
    let seed = closed_port()?
    var held: List[Int] = []
    let c = linux.net_constants()
    var usable = true
    for offset in range(3) {
      let fd = linux.socket(c.AF_INET, c.SOCK_STREAM)?
      match linux.bind(fd, {family: "inet", address: "127.0.0.1", port: seed + offset}) {
        Ok(_) => held += [fd]
        Err(_) => {
          unix.close_fd(fd)?
          usable = false
          break
        }
      }
    }
    if usable {
      linux.listen(held[1], 4)?
      middle = held[1]
      base = seed
      unix.close_fd(held[0])?
      unix.close_fd(held[2])?
      break
    }
    for fd in held {
      unix.close_fd(fd)?
    }
  }
  if middle < 0 {
    test.skip("no three consecutive free loopback ports")
    return
  }
  defer unix.close_fd(middle)
  let high = base + 2
  let ran = run_nc(ctx, ["-vz", "-n", "127.0.0.1", f"{high}-{base}"])?
  assert ran.status == 0
  let want = f"nc: connect to 127.0.0.1 port {base} (tcp) failed: Connection refused\nConnection to 127.0.0.1 {base + 1} port [tcp/*] succeeded!\nnc: connect to 127.0.0.1 port {high} (tcp) failed: Connection refused\n"
  assert ran.stderr == want, ran.stderr

  let shuffled = run_nc(ctx, ["-rvz", "-n", "127.0.0.1", f"{base}-{high}"])?
  assert shuffled.status == 0
  let lines = shuffled.stderr.lines()
  assert lines.len() == 3
  for port in [base, base + 1, high] {
    let refused = f"nc: connect to 127.0.0.1 port {port} (tcp) failed: Connection refused"
    let open = f"Connection to 127.0.0.1 {port} port [tcp/*] succeeded!"
    assert refused in lines or open in lines, shuffled.stderr
  }
}

test test_nc_connects_over_ipv6_loopback { |ctx|
  match tcp_listener("::1") {
    Err(_) => test.skip("no IPv6 loopback in this network namespace")
    Ok(server) => {
      defer unix.close_fd(server.fd)
      let ran = run_nc(ctx, ["-6", "-vz", "-n", "::1", f"{server.port}"])?
      assert ran.status == 0
      assert ran.stderr == f"Connection to ::1 {server.port} port [tcp/*] succeeded!\n", ran.stderr
    }
  }
}

test test_nc_interval_waits_between_ports { |ctx|
  let first = tcp_listener()?
  defer unix.close_fd(first.fd)
  let second = tcp_listener()?
  defer unix.close_fd(second.fd)
  let began = time.now()
  let ran = run_nc(ctx, ["-z", "-n", "-i", "1", "127.0.0.1", f"{first.port}", f"{second.port}"])?
  assert ran.status == 0
  assert time.now() - began >= 1000
}

# A listener whose accept queue is full drops further SYNs, which is how a
# silent peer looks to connect(2) on loopback.
test test_nc_connect_timeout_is_reported_by_name { |ctx|
  let c = linux.net_constants()
  let held = linux.socket(c.AF_INET, c.SOCK_STREAM)?
  defer unix.close_fd(held)
  linux.bind(held, {family: "inet", address: "127.0.0.1", port: 0})?
  linux.listen(held, 0)?
  let port = linux.getsockname(held)?.port
  var fillers: List[Int] = []
  var full = false
  for _ in range(16) {
    let filler = linux.socket(c.AF_INET, c.SOCK_STREAM.bit_or(c.SOCK_NONBLOCK))?
    fillers += [filler]
    match linux.connect(filler, {family: "inet", address: "127.0.0.1", port: port}) {
      Ok(_) => { }
      Err(_) => { }
    }
    if unix.poll_fd(filler, ["writable"], 300)?.is_empty() {
      full = true
      break
    }
  }
  if ! full {
    for filler in fillers {
      unix.close_fd(filler)?
    }
    test.skip("the kernel accepted every queued connection")
    return
  }
  let began = time.now()
  let ran = run_nc(ctx, ["-w", "1", "-v", "-z", "-n", "127.0.0.1", f"{port}"])?
  assert ran.status == 1
  assert time.now() - began >= 1000
  assert ran.stderr == f"nc: connect to 127.0.0.1 port {port} (tcp) failed: Operation timed out\n", ran.stderr
  for filler in fillers {
    unix.close_fd(filler)?
  }
}

test test_nc_listen_fails_when_the_address_is_taken { |ctx|
  let held = tcp_listener()?
  defer unix.close_fd(held.fd)
  let ran = run_nc(ctx, ["-l", "-n", "127.0.0.1", f"{held.port}"])?
  assert ran.status == 1
  assert ran.stderr == "nc: Address in use\n", ran.stderr
}

test test_nc_keep_open_serves_connections_in_turn { |ctx|
  let port = closed_port()?
  let root = test.temp_dir(ctx, name: "nc-keep")?
  let handle = spawn nc_command(ctx, root, "server", ["-v", "-n", "-k", "-l", "127.0.0.1", f"{port}"])?
  await_text(fp"{root}/server.err", "Listening on")?
  for word in ["one\n", "two\n", "three\n"] {
    let client = tcp_client(port)?
    send_all(client, bytes.from_text(word))?
    unix.close_fd(client)?
  }
  var got = ""
  let started = time.now()
  while got != "one\ntwo\nthree\n" and time.now() - started < 10000 {
    got = fp"{root}/server.out".read_text() ?? ""
    time.sleep(20ms)?
  }
  assert got == "one\ntwo\nthree\n", got
  handle.cancel(signal: "TERM", kill_after: 2s)?
  let lines = fp"{root}/server.err".read_text()?.lines()
  assert [l for l in lines if l.starts_with("Connection received on 127.0.0.1 ")].len() == 3
}

test test_nc_receive_limit_ends_the_session { |ctx|
  let port = closed_port()?
  let root = test.temp_dir(ctx, name: "nc-limit")?
  let handle = spawn nc_command(ctx, root, "server", ["-n", "-l", "-W", "1", "127.0.0.1", f"{port}"])?
  # No readiness text without -v: retry until the listener accepts.
  var client = -1
  let started = time.now()
  while client < 0 and time.now() - started < 10000 {
    match tcp_client(port) {
      Ok(fd) => client = fd
      Err(_) => time.sleep(20ms)?
    }
  }
  assert client >= 0
  defer unix.close_fd(client)
  send_all(client, b"one\n")?
  let ran = finished(root, "server", wait handle?)?
  assert ran.status == 0
  assert ran.stdout == b"one\n"
}

test test_nc_crlf_adds_a_carriage_return_before_bare_newlines { |ctx|
  let server = tcp_listener()?
  defer unix.close_fd(server.fd)
  let root = test.temp_dir(ctx, name: "nc-crlf")?
  let handle = spawn nc_command(ctx, root, "client", ["-C", "-N", "-n", "127.0.0.1", f"{server.port}"], b"a\nb\r\nc\n\r\r\nd\n\ne\r")?
  wait_readable(server.fd)?
  let accepted = linux.accept(server.fd)?
  assert read_to_eof(accepted.fd)? == b"a\r\nb\r\nc\r\n\r\r\nd\r\n\r\ne\r"
  unix.close_fd(accepted.fd)?
  let ran = finished(root, "client", wait handle?)?
  assert ran.status == 0
}

test test_nc_detached_stdin_is_not_sent { |ctx|
  let server = tcp_listener()?
  defer unix.close_fd(server.fd)
  let root = test.temp_dir(ctx, name: "nc-detached")?
  let handle = spawn nc_command(ctx, root, "client", ["-d", "-n", "127.0.0.1", f"{server.port}"], b"ignored\n")?
  wait_readable(server.fd)?
  let accepted = linux.accept(server.fd)?
  send_all(accepted.fd, b"hello\n")?
  linux.shutdown(accepted.fd, linux.net_constants().SHUT_WR)?
  # The client's stdin held data, but -d never reads it: the server sees only
  # the end of the stream once the client has finished.
  let ran = finished(root, "client", wait handle?)?
  assert ran.status == 0
  assert ran.stdout == b"hello\n"
  assert read_to_eof(accepted.fd)? == b""
  unix.close_fd(accepted.fd)?
}

test test_nc_quit_timer_ends_the_session_after_stdin_eof { |ctx|
  let server = tcp_listener()?
  defer unix.close_fd(server.fd)
  let root = test.temp_dir(ctx, name: "nc-quit")?
  let began = time.now()
  let handle = spawn nc_command(ctx, root, "client", ["-q", "1", "-n", "127.0.0.1", f"{server.port}"], b"x\n")?
  wait_readable(server.fd)?
  let accepted = linux.accept(server.fd)?
  defer unix.close_fd(accepted.fd)
  let ran = finished(root, "client", wait handle?)?
  let took = time.now() - began
  assert ran.status == 0
  assert took >= 1000, f"{took} {ran.stderr} {ran.stdout.len()}"
  # nc closed its socket: the server sees the line and then end of file.
  assert read_to_eof(accepted.fd)? == b"x\n"
}

test test_nc_idle_timeout_ends_a_quiet_session { |ctx|
  let server = tcp_listener()?
  defer unix.close_fd(server.fd)
  let root = test.temp_dir(ctx, name: "nc-idle")?
  let began = time.now()
  let handle = spawn nc_command(ctx, root, "client", ["-w", "1", "-n", "127.0.0.1", f"{server.port}"])?
  wait_readable(server.fd)?
  let accepted = linux.accept(server.fd)?
  defer unix.close_fd(accepted.fd)
  let ran = finished(root, "client", wait handle?)?
  assert ran.status == 0
  assert time.now() - began >= 1000
}

test test_nc_answers_telnet_negotiation { |ctx|
  let server = tcp_listener()?
  defer unix.close_fd(server.fd)
  let root = test.temp_dir(ctx, name: "nc-telnet")?
  let handle = spawn nc_command(ctx, root, "client", ["-t", "-n", "-N", "127.0.0.1", f"{server.port}"])?
  wait_readable(server.fd)?
  let accepted = linux.accept(server.fd)?
  # IAC DO 1, then IAC WILL 3: refused with WONT and DONT.
  send_all(accepted.fd, bytes.from_ints([255, 253, 1, 255, 251, 3])?)?
  let reply = unix.poll_fd(accepted.fd, ["readable"], 10000)?
  assert ! reply.is_empty()
  var got = b""
  while got.len() < 6 {
    got = bytes.concat([got, unix.read_fd(accepted.fd, 6)?])
  }
  assert got == bytes.from_ints([255, 252, 1, 255, 254, 3])?
  unix.close_fd(accepted.fd)?
  let ran = finished(root, "client", wait handle?)?
  assert ran.stdout == bytes.from_ints([255, 253, 1, 255, 251, 3])?
}

test test_nc_binds_the_requested_source_port { |ctx|
  let server = tcp_listener()?
  defer unix.close_fd(server.fd)
  let source = closed_port()?
  let root = test.temp_dir(ctx, name: "nc-source")?
  let handle = spawn nc_command(ctx, root, "client", ["-n", "-N", "-s", "127.0.0.1", "-p", f"{source}", "127.0.0.1", f"{server.port}"])?
  wait_readable(server.fd)?
  let accepted = linux.accept(server.fd)?
  assert accepted.peer.port == source
  assert accepted.peer.address == "127.0.0.1"
  unix.close_fd(accepted.fd)?
  let ran = finished(root, "client", wait handle?)?
  assert ran.status == 0

  let busy = run_nc(ctx, ["-n", "-z", "-p", f"{server.port}", "127.0.0.1", f"{server.port}"])?
  assert busy.status == 1
  assert busy.stderr == "nc: bind failed: Address in use\n", busy.stderr
  let elsewhere = run_nc(ctx, ["-n", "-s", "10.1.2.3", "-z", "127.0.0.1", f"{server.port}"])?
  assert elsewhere.status == 1
  assert elsewhere.stderr == "nc: bind failed: Address not available\n", elsewhere.stderr
}

test test_nc_udp_client_sends_a_datagram { |ctx|
  let receiver = udp_socket()?
  defer unix.close_fd(receiver.fd)
  let c = linux.net_constants()
  linux.setsockopt_int(receiver.fd, c.SOL_IP, c.IP_RECVTTL, 1)?
  linux.setsockopt_int(receiver.fd, c.SOL_IP, c.IP_RECVTOS, 1)?
  # -M, -T, -b, -I and -O all configure the socket before the first datagram.
  let ran = run_nc(ctx, ["-u", "-v", "-n", "-q", "0", "-M", "17", "-T", "lowdelay", "-b", "-I", "4096", "-O", "4096", "127.0.0.1", f"{receiver.port}"], b"ping\n")?
  assert ran.status == 0
  assert ran.stderr == "", ran.stderr
  wait_readable(receiver.fd)?
  let got = linux.recvfrom(receiver.fd, 64)?
  assert got.data == b"ping\n"
  let ttl = [m for m in got.control if m.level == c.SOL_IP and m.type == c.IP_TTL]
  assert ttl.len() == 1 and bytes.unpack_le(ttl[0].data, 4)? == 17
  let tos = [m for m in got.control if m.level == c.SOL_IP and m.type == c.IP_TOS]
  assert tos.len() == 1 and tos[0].data.byte_at(0) == 16
}

test test_nc_udp_listener_pairs_with_its_first_sender { |ctx|
  let c = linux.net_constants()
  let held = udp_socket()?
  let port = held.port
  unix.close_fd(held.fd)?
  let root = test.temp_dir(ctx, name: "nc-udp")?
  let handle = spawn nc_command(ctx, root, "server", ["-u", "-v", "-n", "-l", "-W", "1", "127.0.0.1", f"{port}"], b"pong\n")?
  await_text(fp"{root}/server.err", "Bound on")?

  let peer = linux.socket(c.AF_INET, c.SOCK_DGRAM)?
  defer unix.close_fd(peer)
  linux.bind(peer, {family: "inet", address: "127.0.0.1", port: 0})?
  let local = linux.getsockname(peer)?
  assert linux.sendto(peer, b"datagram\n", {family: "inet", address: "127.0.0.1", port: port})? == 9
  wait_readable(peer)?
  assert linux.recvfrom(peer, 64)?.data == b"pong\n"

  let ran = finished(root, "server", wait handle?)?
  assert ran.status == 0
  assert ran.stdout == b"datagram\n"
  assert ran.stderr == f"Bound on 127.0.0.1 {port}\nConnection received on 127.0.0.1 {local.port}\n", ran.stderr
}

test test_nc_udp_scan_sends_four_probes_and_detects_closed_ports { |ctx|
  let open = udp_socket()?
  defer unix.close_fd(open.fd)
  let ran = run_nc(ctx, ["-vzu", "-n", "127.0.0.1", f"{open.port}"])?
  assert ran.status == 0
  assert ran.stderr == f"Connection to 127.0.0.1 {open.port} port [udp/*] succeeded!\n", ran.stderr
  for _ in range(4) {
    wait_readable(open.fd)?
    assert linux.recvfrom(open.fd, 16)?.data == b"X"
  }

  let dead = udp_socket()?
  unix.close_fd(dead.fd)?
  let closed = run_nc(ctx, ["-vzu", "-n", "127.0.0.1", f"{dead.port}"])?
  assert closed.status == 1
  assert closed.stderr == ""
}

# The socket path must fit sun_path, which a test's scratch directory can exceed.
proc short_socket_path(name: Str) [env, process, error] -> Result[Str] {
  let pid = process.current_pid()?
  Ok(f"{env.get_or("TMPDIR", "/tmp") ?? "/tmp"}/nc-{pid}-{name}")
}

test test_nc_unix_stream_client_and_listener { |ctx|
  let socket = short_socket_path("stream")?
  if socket.byte_len() > 100 {
    test.skip("the temporary directory socket is too long for a Unix socket")
    return
  }
  defer fp"{socket}".remove()
  let root = test.temp_dir(ctx, name: "nc-unix")?
  let server = spawn nc_command(ctx, root, "server", ["-v", "-U", "-l", socket], b"srv\n")?
  await_text(fp"{root}/server.err", "Listening on")?
  let client = run_nc(ctx, ["-v", "-U", "-N", socket], b"cli\n")?
  assert client.status == 0
  assert client.stdout == b"srv\n"
  assert client.stderr == ""
  let served = finished(root, "server", wait server?)?
  assert served.status == 0
  assert served.stdout == b"cli\n"
  assert served.stderr == f"Bound on {socket}\nListening on {socket}\nConnection received on {socket}\n", served.stderr

  # A scan reports a stale socket as a refusal and a missing one as absent.
  let probe = run_nc(ctx, ["-vzU", socket])?
  assert probe.status == 1
  assert probe.stderr == f"nc: {socket}: Connection refused\n", probe.stderr
  let absent = run_nc(ctx, ["-U", f"{socket}.missing"])?
  assert absent.status == 1
  assert absent.stderr == f"nc: {socket}.missing: No such file or directory\n", absent.stderr
}

test test_nc_unix_datagram_listener_answers_a_client { |ctx|
  let socket = short_socket_path("dgram")?
  if socket.byte_len() > 100 {
    test.skip("the temporary directory socket is too long for a Unix socket")
    return
  }
  defer fp"{socket}".remove()
  let root = test.temp_dir(ctx, name: "nc-unix-dgram")?
  let server = spawn nc_command(ctx, root, "server", ["-v", "-U", "-u", "-l", "-W", "1", socket], b"back\n")?
  await_text(fp"{root}/server.err", "Bound on")?
  let client = run_nc(ctx, ["-U", "-u", "-q", "2", socket], b"dgram")?
  assert client.status == 0
  assert client.stdout == b"back\n"
  let served = finished(root, "server", wait server?)?
  assert served.status == 0
  assert served.stdout == b"dgram"
}
