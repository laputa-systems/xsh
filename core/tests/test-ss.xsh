# Loopback-only tests of `ss`. Every socket belongs to the test process in its
# own network namespace; the applet runs as a child and reads the same table.
# Expected layouts were taken from the iproute2 `ss` on sockets created by an
# identical setup, with the port numbers replaced by the live ones.
use core.lib.net_sockets as sockets

type Ran = {status: Int, stdout: Str, stderr: Str}

# The sockets one test builds. Every port is an ephemeral one of five digits,
# which the expected tables rely on for their padding.
type Scene = {
  listen4: Int, client4: Int, server4: Int,
  listen6: Int, client6: Int, server6: Int,
  udp: Int, udp_client: Int,
  a: Int, ca: Int, b: Int, cb: Int, c: Int, cu: Int,
}

# Runs the applet with fixture name databases, so a port or address never
# turns into a name the host's own files happen to define.
proc run_ss(ctx: TestContext, args: List[Str], hosts_text: Str, services_text: Str) [fs, process, error] -> Result[Ran, Error] {
  let root = test.temp_dir(ctx, name: "ss")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let hosts = fp"{root}/hosts"
  let services = fp"{root}/services"
  hosts.write(hosts_text)?
  services.write(services_text)?
  let script = fp"{ctx.core_dir}/ss.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let variables = {LC_ALL: "C", XSH_EXECUTION_PHRASE: "", XSH_HOSTS_FILE: hosts.display(), XSH_SERVICES_FILE: services.display(), XSH_PROTOCOLS_FILE: services.display()}
  let plan = process.command_argv(ctx.xsh_bin, argv, root, variables, b"", out, err, timeout: 30s)
  let status = process.run(plan)?
  Ok({status: status.shell_code()?, stdout: out.read_text()?, stderr: err.read_text()?})
}

proc ss(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[Ran, Error] {
  run_ss(ctx, args, "", "")
}

proc stream_pair(family: Int, kind: Str, address: Str, backlog: Int) [process, error] -> Result[List[Int], Error] {
  let c = linux.net_constants()
  let listener = linux.socket(family, c.SOCK_STREAM)?
  linux.bind(listener, {family: kind, address: address, port: 0})?
  linux.listen(listener, backlog)?
  let client = linux.socket(family, c.SOCK_STREAM)?
  linux.connect(client, linux.getsockname(listener)?)?
  let accepted = linux.accept(listener)?
  Ok([listener, client, accepted.fd])
}

# Builds the scene: an IPv4 listener with one connection holding 100 unread
# bytes, an IPv6 listener with a connection, and a connected UDP pair.
proc build_scene() [process, error] -> Result[Scene, Error] {
  let c = linux.net_constants()
  let four = stream_pair(c.AF_INET, "inet", "127.0.0.1", 4)?
  let _ = unix.write_fd(four[1], bytes.zero(100)?)?
  let six = stream_pair(c.AF_INET6, "inet6", "::1", 4)?
  let udp = linux.socket(c.AF_INET, c.SOCK_DGRAM)?
  linux.bind(udp, {family: "inet", address: "127.0.0.1", port: 0})?
  let udp_client = linux.socket(c.AF_INET, c.SOCK_DGRAM)?
  linux.connect(udp_client, linux.getsockname(udp)?)?
  let scene: Scene = {
    listen4: four[0], client4: four[1], server4: four[2],
    listen6: six[0], client6: six[1], server6: six[2],
    udp: udp, udp_client: udp_client,
    a: linux.getsockname(four[0])?.port, ca: linux.getsockname(four[1])?.port,
    b: linux.getsockname(six[0])?.port, cb: linux.getsockname(six[1])?.port,
    c: linux.getsockname(udp)?.port, cu: linux.getsockname(udp_client)?.port,
  }
  Ok(scene)
}

proc close_scene(scene: Scene) [process] {
  for fd in [scene.listen4, scene.client4, scene.server4, scene.listen6, scene.client6, scene.server6, scene.udp, scene.udp_client] {
    let _ = unix.close_fd(fd)
  }
}

pure five_digit(scene: Scene) -> Bool {
  [port for port in [scene.a, scene.ca, scene.b, scene.cb, scene.c, scene.cu] if port >= 10000 and port <= 99999].len() == 6
}

pure has(text: Str, part: Str) -> Bool {
  text.find(part) != null
}

# The number of rows `ss -tanH WORDS` prints.
proc rows(ctx: TestContext, words: List[Str]) [fs, process, error] -> Result[Int, Error] {
  let ran = ss(ctx, ["-tanH"].extend(words))?
  assert ran.status == 0, ran.stderr
  Ok(ran.stdout.lines().len())
}

# The data rows of a table in a stable order: the kernel's order among
# established sockets of one hash chain is not part of the contract.
pure sorted_lines(items: List[Str]) -> List[Str] {
  items |> sort-by .
}

pure only(scene: Scene, port: Int) -> Str {
  f"sport = :{port} or dport = :{port}"
}

test test_ss_lists_listeners_in_the_reference_layout { |ctx|
  let scene = build_scene()?
  defer close_scene(scene)
  if !five_digit(scene) { test.skip("the ephemeral port range holds ports below 10000"); return }
  let ran = ss(ctx, ["-tln", f"sport = :{scene.a} or sport = :{scene.b}"])?
  assert ran.status == 0, ran.stderr
  assert ran.stdout.lines() == [
    "State  Recv-Q Send-Q Local Address:Port  Peer Address:Port",
    f"LISTEN 0      4          127.0.0.1:{scene.a}      0.0.0.0:*",
    f"LISTEN 0      4              [::1]:{scene.b}         [::]:*",
  ]
}

test test_ss_shows_established_connections_with_their_queues { |ctx|
  let scene = build_scene()?
  defer close_scene(scene)
  if !five_digit(scene) { test.skip("the ephemeral port range holds ports below 10000"); return }
  let ran = ss(ctx, ["-tn", only(scene, scene.a)])?
  assert ran.status == 0, ran.stderr
  let lines = ran.stdout.lines()
  assert lines[0] == "State Recv-Q Send-Q Local Address:Port  Peer Address:Port"
  assert sorted_lines(lines |> drop(1)) == sorted_lines([
    f"ESTAB 0      0          127.0.0.1:{scene.ca}    127.0.0.1:{scene.a}",
    f"ESTAB 100    0          127.0.0.1:{scene.a}    127.0.0.1:{scene.ca}",
  ])
}

test test_ss_header_suppression_and_ipv6_alignment { |ctx|
  let scene = build_scene()?
  defer close_scene(scene)
  if !five_digit(scene) { test.skip("the ephemeral port range holds ports below 10000"); return }
  let ran = ss(ctx, ["-tanH", only(scene, scene.b)])?
  assert ran.status == 0, ran.stderr
  let lines = ran.stdout.lines()
  assert lines[0] == f"LISTEN 0      4      [::1]:{scene.b}  [::]:*"
  assert sorted_lines(lines |> drop(1)) == sorted_lines([
    f"ESTAB  0      0      [::1]:{scene.b} [::1]:{scene.cb}",
    f"ESTAB  0      0      [::1]:{scene.cb} [::1]:{scene.b}",
  ])
}

test test_ss_udp_unconnected_and_connected { |ctx|
  let scene = build_scene()?
  defer close_scene(scene)
  if !five_digit(scene) { test.skip("the ephemeral port range holds ports below 10000"); return }
  let ran = ss(ctx, ["-uan", only(scene, scene.c)])?
  assert ran.status == 0, ran.stderr
  let lines = ran.stdout.lines()
  assert lines[0] == "State  Recv-Q Send-Q Local Address:Port  Peer Address:Port"
  assert sorted_lines(lines |> drop(1)) == sorted_lines([
    f"UNCONN 0      0          127.0.0.1:{scene.c}      0.0.0.0:*",
    f"ESTAB  0      0          127.0.0.1:{scene.cu}    127.0.0.1:{scene.c}",
  ])
}

test test_ss_unix_sockets_print_inodes_as_ports { |ctx|
  let c = linux.net_constants()
  let root = test.temp_dir(ctx, name: "ss-unix")?
  let socket_path = fp"{root}/ss.sock"
  # sun_path holds 107 bytes; a long scratch directory cannot host the socket.
  if socket_path.display().byte_len() > 100 { test.skip("the scratch path is too long for a unix socket"); return }
  let server = linux.socket(c.AF_UNIX, c.SOCK_STREAM)?
  defer unix.close_fd(server)
  linux.bind(server, {family: "unix", address: socket_path.display()})?
  linux.listen(server, 5)?
  let client = linux.socket(c.AF_UNIX, c.SOCK_STREAM)?
  defer unix.close_fd(client)
  linux.connect(client, {family: "unix", address: socket_path.display()})?
  let accepted = linux.accept(server)?
  defer unix.close_fd(accepted.fd)
  let ran = ss(ctx, ["-xan", f"src {socket_path.display()}"])?
  assert ran.status == 0, ran.stderr
  let lines = ran.stdout.lines()
  assert lines.len() == 3, ran.stdout
  assert lines[0].starts_with("Netid State  Recv-Q Send-Q ")
  assert has(lines[0], "Local Address:Port") and lines[0].ends_with("Peer Address:Port"), lines[0]
  # Rows: netid, state, queues, name, inode, the peer name "*", peer inode.
  var listening = 0
  var connected = 0
  for row in lines |> drop(1) {
    let fields = row.fields()
    assert fields.len() == 8, row
    assert fields[0] == "u_str"
    assert fields[4] == socket_path.display()
    assert rx"^[0-9]+$".matches(fields[5]) and rx"^[0-9]+$".matches(fields[7])
    assert fields[6] == "*"
    if fields[1] == "LISTEN" {
      listening += 1
      assert fields[2] == "0" and fields[7] == "0"
      assert row.starts_with("u_str LISTEN 0      5      ")
    } else {
      assert fields[1] == "ESTAB"
      connected += 1
    }
  }
  assert listening == 1 and connected == 1
  # Without -a the listener is hidden, and the State column goes with a
  # single-state selection.
  let only_listener = ss(ctx, ["-xlnH", f"src {socket_path.display()}"])?
  assert only_listener.stdout.lines().len() == 1
  assert only_listener.stdout.starts_with("u_str LISTEN 0      5      ")
}

test test_ss_single_state_filters_drop_the_state_column { |ctx|
  let scene = build_scene()?
  defer close_scene(scene)
  if !five_digit(scene) { test.skip("the ephemeral port range holds ports below 10000"); return }
  let ran = ss(ctx, ["-tn", "state", "listening", "sport", "=", f":{scene.a}"])?
  assert ran.status == 0, ran.stderr
  assert ran.stdout.lines() == [
    "Recv-Q Send-Q Local Address:Port  Peer Address:Port",
    f"0      4          127.0.0.1:{scene.a}      0.0.0.0:*",
  ]
  # Closing the client side leaves the accepted socket in CLOSE-WAIT and the
  # closer in FIN-WAIT-2; the state keywords select each.
  let c = linux.net_constants()
  linux.shutdown(scene.client4, c.SHUT_WR)?
  assert unix.poll_fd(scene.server4, ["readable"], 1000)? == ["readable"]
  let waiting = ss(ctx, ["-tnH", "state", "close-wait", "sport", "=", f":{scene.a}"])?
  assert waiting.stdout.lines().len() == 1, waiting.stdout
  assert waiting.stdout.fields()[0] == "101", waiting.stdout
  let excluded = ss(ctx, ["-tn", "exclude", "established", "exclude", "listening", "sport", "=", f":{scene.a}"])?
  assert excluded.stdout.lines()[0].starts_with("State      Recv-Q"), excluded.stdout
  let bad = ss(ctx, ["-tn", "state", "bogus"])?
  assert bad.status == 255
  assert bad.stderr.lines()[0] == "ss: wrong state name: bogus"
  let incomplete = ss(ctx, ["-tn", "state"])?
  assert incomplete.status == 255
  assert incomplete.stderr.lines()[0] == "Command line is not complete. Try option \"help\""
}

test test_ss_no_queues_and_oneline_layouts { |ctx|
  let scene = build_scene()?
  defer close_scene(scene)
  if !five_digit(scene) { test.skip("the ephemeral port range holds ports below 10000"); return }
  let quiet = ss(ctx, ["-tnQ", only(scene, scene.a)])?
  assert quiet.status == 0, quiet.stderr
  assert quiet.stdout.lines()[0] == "State Local Address:Port  Peer Address:Port"
  assert sorted_lines(quiet.stdout.lines() |> drop(1)) == sorted_lines([
    f"ESTAB     127.0.0.1:{scene.ca}    127.0.0.1:{scene.a}",
    f"ESTAB     127.0.0.1:{scene.a}    127.0.0.1:{scene.ca}",
  ])
  let both = ss(ctx, ["-tnHQ", f"sport = :{scene.a} and dport = :{scene.ca}"])?
  assert both.stdout == f"ESTAB 127.0.0.1:{scene.a} 127.0.0.1:{scene.ca}\n"
  # -m prints the memory line below each socket; -O keeps it on the socket's
  # own line.
  let memory = ss(ctx, ["-tnHm", f"sport = :{scene.a} and dport = :{scene.ca}"])?
  let memory_lines = memory.stdout.lines()
  assert memory_lines.len() == 2, memory.stdout
  assert memory_lines[1].starts_with("\t skmem:(r"), memory.stdout
  let oneline = ss(ctx, ["-tnHmO", f"sport = :{scene.a} and dport = :{scene.ca}"])?
  assert oneline.stdout.lines().len() == 1, oneline.stdout
  assert rx"^ESTAB 100    0      127\.0\.0\.1:[0-9]{5} 127\.0\.0\.1:[0-9]{5} skmem:\(r[0-9]+,rb[0-9]+,t[0-9]+,tb[0-9]+,f[0-9]+,w[0-9]+,o[0-9]+,bl[0-9]+,d[0-9]+\)\n$".matches(oneline.stdout), oneline.stdout
}

test test_ss_extended_and_timer_text { |ctx|
  let scene = build_scene()?
  defer close_scene(scene)
  if !five_digit(scene) { test.skip("the ephemeral port range holds ports below 10000"); return }
  let ran = ss(ctx, ["-tneH", f"sport = :{scene.a} and dport = :{scene.ca}"])?
  assert ran.status == 0, ran.stderr
  assert rx"^ESTAB 100    0      127\.0\.0\.1:[0-9]{5} 127\.0\.0\.1:[0-9]{5} uid:[0-9]+ ino:[0-9]+ sk:[0-9a-f]+ cgroup:\S+ <->\n$".matches(ran.stdout) or rx"^ESTAB 100    0      127\.0\.0\.1:[0-9]{5} 127\.0\.0\.1:[0-9]{5} ino:[0-9]+ sk:[0-9a-f]+ cgroup:\S+ <->\n$".matches(ran.stdout), ran.stdout
  let six = ss(ctx, ["-tlneH", f"sport = :{scene.b}"])?
  assert six.stdout.lines().len() == 1
  assert has(six.stdout, " v6only:1 <->"), six.stdout

  # A half-closed connection shows its shutdown directions and, with -o, the
  # armed keepalive timer.
  let c = linux.net_constants()
  linux.setsockopt_int(scene.client4, c.SOL_SOCKET, c.SO_KEEPALIVE, 1)?
  linux.shutdown(scene.client4, c.SHUT_WR)?
  let half = ss(ctx, ["-tneoH", f"sport = :{scene.ca} and dport = :{scene.a}"])?
  assert half.status == 0, half.stderr
  assert rx"timer:\(keepalive,[0-9a-z.]+,0\) .* <--\n$".matches(half.stdout), half.stdout
}

test test_ss_tcp_info_text_follows_the_reference_order { |ctx|
  let scene = build_scene()?
  defer close_scene(scene)
  if !five_digit(scene) { test.skip("the ephemeral port range holds ports below 10000"); return }
  let listener = ss(ctx, ["-tlniH", f"sport = :{scene.a} and dport = :0"])?
  assert listener.status == 0, listener.stderr
  let lines = listener.stdout.lines()
  assert lines.len() == 2, listener.stdout
  assert rx"^\t [a-z]+ cwnd:[0-9]+( unacked:[0-9]+)?$".matches(lines[1]), listener.stdout

  let connection = ss(ctx, ["-tniH", f"sport = :{scene.ca} and dport = :{scene.a}"])?
  let info = connection.stdout.lines()[1]
  assert rx"^\t [a-z]+ wscale:[0-9]+,[0-9]+ rto:[0-9.]+ rtt:[0-9.]+/[0-9.]+ .*mss:[0-9]+ pmtu:[0-9]+ rcvmss:[0-9]+ advmss:[0-9]+ cwnd:[0-9]+".matches(info), info
  assert has(info, " bytes_sent:100 "), info
  assert has(info, " send ") and has(info, " pacing_rate "), info
  # -o adds the negotiated option flags in front of the algorithm name.
  let options = ss(ctx, ["-tnioH", f"sport = :{scene.ca} and dport = :{scene.a}"])?
  assert options.stdout.lines()[1].starts_with("\t ts sack "), options.stdout
}

test test_ss_process_column_names_the_owner { |ctx|
  let scene = build_scene()?
  defer close_scene(scene)
  if !five_digit(scene) { test.skip("the ephemeral port range holds ports below 10000"); return }
  let me = process.current_pid()?
  let entries: List[ProcessEntry] = process.list()? |> where .pid == me |> collect
  let name = entries[0].command
  let ran = ss(ctx, ["-tlnp", f"sport = :{scene.a} and dport = :0"])?
  assert ran.status == 0, ran.stderr
  let lines = ran.stdout.lines()
  assert lines[0] == "Recv-Q Send-Q Local Address:Port  Peer Address:PortProcess" or lines[0].ends_with("Port Process") or lines[0].ends_with("PortProcess"), lines[0]
  assert lines.len() == 2, ran.stdout
  assert lines[1].ends_with(f"users:((\"{name}\",pid={me},fd={scene.listen4}))"), lines[1]
  # A socket held by two descriptors lists both, the later one first.
  let duplicate = 200
  unix.dup_fd(scene.listen4, duplicate)?
  defer unix.close_fd(duplicate)
  let twice = ss(ctx, ["-tlnHp", f"sport = :{scene.a} and dport = :0"])?
  let low = scene.listen4
  let high = duplicate
  let first = if high > low { high } else { low }
  let second = if high > low { low } else { high }
  # Children spawned meanwhile inherit the duplicate descriptor and appear
  # too, so only the adjacent pair of this process is pinned.
  assert has(twice.stdout, f"(\"{name}\",pid={me},fd={first}),(\"{name}\",pid={me},fd={second})"), twice.stdout
}

test test_ss_summary_reports_the_kernel_counters { |ctx|
  let scene = build_scene()?
  defer close_scene(scene)
  let ran = ss(ctx, ["-s"])?
  assert ran.status == 0, ran.stderr
  let lines = ran.stdout.lines()
  assert rx"^Total: [0-9]+$".matches(lines[0]), lines[0]
  assert rx"^TCP:   [0-9]+ \(estab [0-9]+, closed [0-9]+, orphaned [0-9]+, timewait [0-9]+\)$".matches(lines[1]), lines[1]
  assert lines[2] == ""
  assert lines[3] == "Transport Total     IP        IPv6"
  for kind in ["RAW", "UDP", "TCP", "INET", "FRAG"] {
    let row = lines |> where .starts_with(kind + "\t  ") |> collect
    assert row.len() == 1, kind
    assert rx"^[A-Z]+\t  [0-9]+ +[0-9]+ +[0-9]+ *$".matches(row[0]), row[0]
  }
  # The counters agree with the dump: the scene's three TCP sockets per
  # family are in the hashed totals (the IP column counts IPv4 sockets).
  let tcp_row = (lines |> where .starts_with("TCP\t  ") |> collect)[0].fields()
  assert tcp_row[2] as Int >= 3 and tcp_row[3] as Int >= 3
  # -s with a table selection also prints the listing.
  let both = ss(ctx, ["-s", "-tlnH", f"sport = :{scene.a}"])?
  assert has(both.stdout, "Transport Total"), both.stdout
  assert both.stdout.lines()[both.stdout.lines().len() - 1].starts_with("LISTEN"), both.stdout
}

test test_ss_resolves_names_from_the_configured_databases { |ctx|
  let scene = build_scene()?
  defer close_scene(scene)
  if !five_digit(scene) { test.skip("the ephemeral port range holds ports below 10000"); return }
  let hosts = "127.0.0.1\txsh-local other\n::1\txsh-local6\n"
  let services = f"# fixture\nxshsvc\t{scene.a}/tcp\talias\n"
  let filter = f"sport = :{scene.a} and dport = :0"
  let resolved = run_ss(ctx, ["-tlr", filter], hosts, services)?
  assert resolved.status == 0, resolved.stderr
  assert has(resolved.stdout.lines()[1], "xsh-local:xshsvc"), resolved.stdout
  # -n keeps ports numeric; hosts still resolve with -r.
  let numeric = run_ss(ctx, ["-tlnr", filter], hosts, services)?
  assert has(numeric.stdout.lines()[1], f"xsh-local:{scene.a}"), numeric.stdout
  let plain = run_ss(ctx, ["-tl", filter], hosts, services)?
  assert has(plain.stdout.lines()[1], "127.0.0.1:xshsvc"), plain.stdout
  # The service name is accepted in filters, and an unknown one fails.
  let by_name = run_ss(ctx, ["-tlnH", "sport", "=", ":xshsvc"], hosts, services)?
  assert by_name.stdout.lines().len() == 1, by_name.stdout
  let unknown = run_ss(ctx, ["-tln", "sport", "=", ":no-such-service"], hosts, services)?
  assert unknown.status == 1
  assert unknown.stderr.lines()[0] == "Error: \"no-such-service\" does not look like a port."
}

test test_ss_filter_expressions_combine_ports_and_prefixes { |ctx|
  let scene = build_scene()?
  defer close_scene(scene)
  if !five_digit(scene) { test.skip("the ephemeral port range holds ports below 10000"); return }
  let a = scene.a
  # The listener, both ends of the connection: three IPv4 rows on port a.
  assert rows(ctx, ["sport", "=", f":{a}", "or", "dport", "=", f":{a}"])? == 3
  assert rows(ctx, ["(", "sport", "=", f":{a}", "or", "dport", "=", f":{a}", ")", "and", "src", "127.0.0.1"])? == 3
  assert rows(ctx, ["sport", "=", f":{a}", "and", "dport", "=", ":0"])? == 1
  assert rows(ctx, ["sport", "=", f":{a}", "and", "not", "dport", "=", ":*"])? == 0
  assert rows(ctx, ["sport", "=", f":{a}", "dport", ">", ":1"])? == 1
  assert rows(ctx, ["sport", ">=", f":{a}", "and", "sport", "<=", f":{a}"])? == 2
  assert rows(ctx, ["sport", "ne", f":{a}", "and", "dport", "ne", f":{a}", "and", "sport", "=", f":{scene.b}"])? == 2
  assert rows(ctx, ["src", "127.0.0.0/8", "sport", "=", f":{a}"])? == 2
  assert rows(ctx, ["src", "[::1]:" + f"{scene.b}"])? == 2
  assert rows(ctx, ["dst", f"*:{a}"])? == 1
  assert rows(ctx, ["src", f"inet:127.0.0.1", "sport", "=", f":{a}"])? == 2
  assert rows(ctx, ["src", "10.0.0.0/8", "sport", "=", f":{a}"])? == 0
  assert rows(ctx, ["!", "sport", "=", f":{a}", "and", "sport", "=", f":{scene.b}"])? == 2
  assert rows(ctx, ["autobound", "sport", "=", f":{a}"])? == 2
  assert rows(ctx, ["src", f"unix:/nonexistent"])? == 0
  let not_a_prefix = ss(ctx, ["-tn", "src", "bogus"])?
  assert not_a_prefix.status == 1
  assert not_a_prefix.stderr.lines() == ["Error: an inet prefix is expected rather than \"bogus\".", "Cannot parse dst/src address."]
  let loose_word = ss(ctx, ["-tn", "127.0.0.1"])?
  assert loose_word.status == 255
  let unbalanced = ss(ctx, ["-tn", "(", "sport", "=", ":1"])?
  assert unbalanced.status == 255
  let device = ss(ctx, ["-tn", "dev", "no-such-device0"])?
  assert device.status == 1
  assert device.stderr.lines()[0] == "Cannot parse device."
}

test test_ss_table_and_family_selection { |ctx|
  let scene = build_scene()?
  defer close_scene(scene)
  if !five_digit(scene) { test.skip("the ephemeral port range holds ports below 10000"); return }
  let port_filter = ["sport", "=", f":{scene.a}", "or", "sport", "=", f":{scene.b}", "or", "sport", "=", f":{scene.c}"]
  # Several tables bring the Netid column back, in unix, raw, udp, tcp order.
  let mixed = ss(ctx, ["-tulnH"].extend(port_filter))?
  assert mixed.status == 0, mixed.stderr
  let kinds = [row.fields()[0] for row in mixed.stdout.lines()]
  assert kinds == ["udp", "tcp", "tcp"], mixed.stdout
  let via_query = ss(ctx, ["-A", "tcp,udp", "-lnH"].extend(port_filter))?
  assert via_query.stdout == mixed.stdout
  let long_form = ss(ctx, ["--query=udp,tcp", "--listening", "--numeric", "--no-header"].extend(port_filter))?
  assert long_form.stdout == mixed.stdout
  let v4 = ss(ctx, ["-4", "-tlnH"].extend(port_filter))?
  assert v4.stdout.lines().len() == 1 and has(v4.stdout, "127.0.0.1"), v4.stdout
  let v6 = ss(ctx, ["-6", "-tlnH"].extend(port_filter))?
  assert v6.stdout.lines().len() == 1 and has(v6.stdout, "[::1]"), v6.stdout
  let family = ss(ctx, ["-f", "inet6", "-tlnH"].extend(port_filter))?
  assert family.stdout == v6.stdout
  let abbreviated = ss(ctx, ["--tc", "--lis", "--num", "--no-h"].extend(port_filter))?
  assert abbreviated.stdout.lines().len() == 2, abbreviated.stdout
  let unknown_table = ss(ctx, ["-A", "bogus"])?
  assert unknown_table.status == 255
  assert unknown_table.stderr.lines()[0] == "ss: \"bogus\" is illegal socket table id"
}

test test_ss_refuses_what_it_cannot_do { |ctx|
  let refused = ["-K", "--kill", "-D", "-E", "-0", "-M", "-S", "-d", "-T", "-b", "-B", "--tos", "--cgroup", "--packet", "--vsock", "-F", "--inet-sockopt"]
  for option in refused {
    let args = if option in ["-D", "-F"] { [option, "/nonexistent"] } else { [option] }
    let ran = ss(ctx, args)?
    assert ran.status == 255, f"{option}: {ran.stdout}"
    assert ran.stdout == "", option
    assert rx"^ss: option '[^']+' is not supported: ".matches(ran.stderr.lines()[0]), f"{option}: {ran.stderr}"
  }
  let packet = ss(ctx, ["-A", "packet"])?
  assert packet.status == 255
  let unknown_family = ss(ctx, ["-f", "link"])?
  assert unknown_family.status == 255
  let invalid = ss(ctx, ["-X"])?
  assert invalid.status == 255
  assert invalid.stderr.lines()[0] == "ss: invalid option -- 'X'"
  let unrecognized = ss(ctx, ["--no-such-option"])?
  assert unrecognized.status == 255
  assert unrecognized.stderr.lines()[0] == "ss: unrecognized option '--no-such-option'"
  let ambiguous = ss(ctx, ["--no"])?
  assert ambiguous.status == 255
  # SELinux contexts fail explicitly instead of being ignored.
  let context = ss(ctx, ["-tZ"])?
  assert context.status == 1
  assert context.stdout == ""
  # --help and --version work and succeed.
  let help = ss(ctx, ["--help"])?
  assert help.status == 0 and help.stdout.starts_with("Usage: ss [ OPTIONS ]")
  let version = ss(ctx, ["-V"])?
  assert version.status == 0 and version.stdout.starts_with("ss utility")
}

test test_net_sockets_ipv6_text_matches_inet_ntop { |ctx|
  let zero = bytes.zero(16)?
  assert sockets.ipv6_text(zero) == "::"
  assert sockets.ipv6_text(bytes.from_ints([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1])?) == "::1"
  assert sockets.ipv6_text(bytes.from_ints([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 127, 0, 0, 1])?) == "::ffff:127.0.0.1"
  assert sockets.ipv6_text(bytes.from_ints([32, 1, 13, 184, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1])?) == "2001:db8::1"
  # The longest run compresses; a single zero group stays.
  assert sockets.ipv6_text(bytes.from_ints([32, 1, 13, 184, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1])?) == "2001:db8:0:1::1"
  assert sockets.ipv6_text(bytes.from_ints([32, 1, 13, 184, 0, 1, 0, 2, 0, 3, 0, 4, 0, 5, 0, 6])?) == "2001:db8:1:2:3:4:5:6"
  assert sockets.ipv6_text(bytes.from_ints([254, 128, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])?) == "fe80::"
  assert sockets.ipv4_text(bytes.from_ints([10, 0, 200, 7])?) == "10.0.200.7"
  assert sockets.endpoint_address(zero, sockets.AF_INET6, false) == "*"
  assert sockets.endpoint_address(zero, sockets.AF_INET6, true) == "::"
  assert sockets.endpoint_address(bytes.zero(4)?, sockets.AF_INET, false) == "0.0.0.0"
}

test test_net_sockets_counter_text_follows_the_reference_tool { |ctx|
  # A tcp_info of 248 bytes with the fields a loopback connection sets:
  # options ts+sack+wscale, wscale 10/10, rto 201 ms, mss 32768, rtt 23 us,
  # rttvar 11 us, cwnd 10, notsent 5.
  let info = bytes.concat([
    bytes.from_ints([0, 0, 0, 0, 0, 7, 170, 0])?, bytes.pack_le(201000, 4)?, bytes.zero(4)?,
    bytes.pack_le(32768, 4)?, bytes.zero(48)?, bytes.pack_le(23, 4)?, bytes.pack_le(11, 4)?,
    bytes.zero(4)?, bytes.pack_le(10, 4)?, bytes.zero(60)?, bytes.pack_le(5, 4)?, bytes.zero(100)?,
  ])
  assert info.len() == 248
  let text = sockets.tcp_info_text(info, "cubic", sockets.TCP_ESTABLISHED, false)
  # 10 * 32768 * 8000000 / 23 bits per second, rounded to a whole number.
  assert text == "cubic wscale:10,10 rto:201 rtt:0.023/0.011 mss:32768 cwnd:10 send 113975652174bps notsent:5", text
  let flagged = sockets.tcp_info_text(info, "cubic", sockets.TCP_ESTABLISHED, true)
  assert flagged.starts_with("ts sack cubic wscale:10,10 "), flagged
  assert sockets.tcp_info_text(b"", null, sockets.TCP_TIME_WAIT, true) == ""
  assert sockets.timer_text(0, 5000, 0) == null
  assert sockets.timer_text(1, 200, 1) == "timer:(on,200ms,1)"
  assert sockets.timer_text(2, 7140000, 0) == "timer:(keepalive,119min,0)"
  assert sockets.timer_text(3, 59000, 0) == "timer:(timewait,59sec,0)"
  assert sockets.timer_text(4, 1500, 2) == "timer:(persist,1.500ms,2)"
  let memory = bytes.concat([bytes.pack_le(0, 4)?, bytes.pack_le(131072, 4)?, bytes.pack_le(0, 4)?, bytes.pack_le(2626560, 4)?, bytes.zero(20)?])
  assert sockets.skmem_text(memory) == "skmem:(r0,rb131072,t0,tb2626560,f0,w0,o0,bl0,d0)"
  assert sockets.hex(0) == "0" and sockets.hex(255) == "ff" and sockets.hex(65536) == "10000"
}

test test_net_sockets_summary_counts_the_scene { |ctx|
  let scene = build_scene()?
  defer close_scene(scene)
  let counts = sockets.summary()?
  assert counts.tcp4 >= 3 and counts.used >= counts.tcp4
  assert counts.udp4 >= 1
  assert counts.allocated >= counts.tcp4
  let ports = sockets.local_port_range()
  assert 0 <= ports.low and ports.low <= ports.high and ports.high <= 65535
}

test test_net_sockets_resolve_cgroup_ids_through_the_unified_hierarchy { |ctx|
  # The cgroup2 root is its own id and names itself "/"; a path that is not a
  # directory of the hierarchy has no id.
  assert sockets.cgroup_id("/definitely/not/a/cgroup") == null
  if let root = sockets.cgroup_id("") {
    assert sockets.cgroup_names().get(f"{root}")? == "/"
  }
}

test test_net_sockets_raw_dump_falls_back_to_procfs_only_without_a_handler { |ctx|
  # A kernel without raw_diag ends the dump with ENOENT. Raw sockets then come
  # from procfs, while the same ending for any other protocol is an error.
  let fixture = test.temp_file(ctx, name: "diag.jsonl", contents: b"")?
  let log = test.temp_file(ctx, name: "diag.log", contents: b"")?
  fixture.write(json.encode_lines([{op: "netlink_request", type: 20, errno: 2}])?)
  test.linux_fake(ctx, {netlink_fixture: fixture, log: log})?
  let channel = sockets.open()?
  defer unix.close_fd(channel)
  let raw = sockets.collect_inet(channel, sockets.AF_INET, 255, 4095, false, false)?
  for item in raw { assert item.netid == "raw" }
  match sockets.collect_inet(channel, sockets.AF_INET, 17, 4095, false, false) {
    Ok(_) => assert false, "a dump without a handler must fail for udp"
    Err(failure) => assert failure.errno == 2
  }
}
