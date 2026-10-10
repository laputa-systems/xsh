# Loopback-only tests of the Linux socket, netlink, and ioctl boundary. Every
# socket lives in this process's own network namespace and nothing here
# reconfigures an interface.

pure errno_of(result: Result[Any, Error]) -> Int {
  if let Err(failure) = result {
    return failure.errno ?? -1
  } else {
    return 0
  }
}

proc loopback_udp() [process, error] -> Result[Int, Error] {
  let c = linux.net_constants()
  let fd = linux.socket(c.AF_INET, c.SOCK_DGRAM)?
  linux.bind(fd, {family: "inet", address: "127.0.0.1", port: 0})?
  Ok(fd)
}

type Attribute = {kind: Int, data: Bytes}

# Splits netlink attributes that follow a fixed header of `start` bytes.
proc attributes(payload: Bytes, start: Int) [error] -> Result[List[Attribute]] {
  var found: List[Attribute] = []
  var offset = start
  while offset + 4 <= payload.len() {
    let length = bytes.unpack_le(payload, 2, offset)?
    let kind = bytes.unpack_le(payload, 2, offset + 2)?
    if length < 4 or offset + length > payload.len() {
      break
    }
    found += [{kind: kind, data: payload.slice(offset + 4, length - 4)}]
    offset += (length + 3) / 4 * 4
  }
  Ok(found)
}

# The host's own netlink snapshot, retried because a loaded or sandboxed
# kernel can refuse part of a multi-request dump; the comparisons below need
# a snapshot taken without a failed object.
proc settled_dump() [process, error] -> Result[LinuxNetworkDump] {
  var attempt = 0
  while attempt < 20 {
    let dump = linux.network_dump()?
    if dump.state == "complete" {
      return Ok(dump)
    }
    attempt += 1
  }
  test.skip("linux.network_dump never completed on this host")?
  Ok(linux.network_dump()?)
}

proc interface_names(replies: List[LinuxNetlinkMessage]) [error] -> Result[List[Str]] {
  var names: List[Str] = []
  for reply in replies {
    for attribute in attributes(reply.payload, 16)? {
      if attribute.kind == 3 {
        names += [attribute.data.slice(0, attribute.data.len() - 1).utf8()?]
      }
    }
  }
  Ok(names)
}

# A `struct ifreq` name field: the interface name padded to IFNAMSIZ.
proc ifname(name: Str) [error] -> Result[Bytes] {
  bytes.concat([bytes.from_text(name), bytes.zero(16 - name.byte_len())?])
}

test test_net_constants_name_the_kernel_abi {
  let c = linux.net_constants()
  assert c.AF_INET == 2 and c.AF_INET6 == 10 and c.AF_NETLINK == 16
  assert c.SOCK_STREAM == 1 and c.SOCK_DGRAM == 2 and c.SOCK_RAW == 3
  assert c.SOCK_CLOEXEC == 0o2000000 and c.SOCK_NONBLOCK == 0o4000
  assert c.NLM_F_DUMP == c.NLM_F_ROOT.bit_or(c.NLM_F_MATCH)
  assert c.SIOCGIFFLAGS == 35091 and c.SIOCETHTOOL == 35142
  assert c.IFREQ_SIZE == 40 and c.ARPREQ_SIZE == 68
  assert c.ICMP_FILTER == 1 and c.TCP_NODELAY == 1 and c.IPV6_V6ONLY == 26
}

test test_tcp_echo_over_loopback {
  let c = linux.net_constants()
  let server = linux.socket(c.AF_INET, c.SOCK_STREAM)?
  defer unix.close_fd(server)
  linux.setsockopt_int(server, c.SOL_SOCKET, c.SO_REUSEADDR, 1)?
  assert linux.getsockopt_int(server, c.SOL_SOCKET, c.SO_REUSEADDR)? == 1
  assert linux.getsockopt_int(server, c.SOL_SOCKET, c.SO_ACCEPTCONN)? == 0
  linux.bind(server, {family: "inet", address: "127.0.0.1", port: 0})?
  linux.listen(server, 4)?
  assert linux.getsockopt_int(server, c.SOL_SOCKET, c.SO_ACCEPTCONN)? == 1
  let local = linux.getsockname(server)?
  assert local.family == "inet" and local.address == "127.0.0.1"
  assert local.port > 0
  assert local.raw.len() == 16

  let client = linux.socket(c.AF_INET, c.SOCK_STREAM)?
  defer unix.close_fd(client)
  linux.setsockopt_int(client, c.IPPROTO_TCP, c.TCP_NODELAY, 1)?
  linux.connect(client, local)?
  assert linux.getpeername(client)? == local

  let accepted = linux.accept(server)?
  defer unix.close_fd(accepted.fd)
  assert accepted.fd > 2
  assert accepted.peer == linux.getsockname(client)?

  assert unix.write_fd(client, bytes.from_text("hello, socket"))? == 13
  assert unix.poll_fd(accepted.fd, ["readable"], 1000)? == ["readable"]
  let request = unix.read_fd(accepted.fd, 64)?
  assert request == bytes.from_text("hello, socket")
  assert linux.sendto(accepted.fd, request)? == 13

  let reply = linux.recvfrom(client, 64)?
  assert reply.data == request
  assert reply.address.family == "unspec"
  assert reply.control == []

  linux.shutdown(client, c.SHUT_WR)?
  assert unix.read_fd(accepted.fd, 64)?.is_empty()
}

test test_udp_exchange_reports_sender_truncation_and_ttl {
  let c = linux.net_constants()
  let receiver = loopback_udp()?
  defer unix.close_fd(receiver)
  let sender = loopback_udp()?
  defer unix.close_fd(sender)
  linux.setsockopt_int(receiver, c.SOL_IP, c.IP_RECVTTL, 1)?
  linux.setsockopt_int(sender, c.SOL_IP, c.IP_TTL, 17)?
  assert linux.getsockopt_int(sender, c.SOL_IP, c.IP_TTL)? == 17

  let target = linux.getsockname(receiver)?
  assert linux.sendto(sender, bytes.from_text("datagram"), target)? == 8
  assert unix.poll_fd(receiver, ["readable"], 1000)? == ["readable"]
  let got = linux.recvfrom(receiver, 4)?
  assert got.data == bytes.from_text("data")
  assert got.flags.bit_and(c.MSG_TRUNC) != 0
  assert got.address == linux.getsockname(sender)?

  let ttl = [m for m in got.control if m.level == c.SOL_IP and m.type == c.IP_TTL]
  assert ttl.len() == 1
  assert bytes.unpack_le(ttl[0].data, 4)? == 17

  # Without MSG_TRUNC in the buffer the whole datagram arrives untruncated.
  assert linux.sendto(sender, bytes.from_text("whole"), target)? == 5
  let whole = linux.recvfrom(receiver, 64)?
  assert whole.data == bytes.from_text("whole")
  assert whole.flags.bit_and(c.MSG_TRUNC) == 0
}

test test_udp_error_queue_reports_the_icmp_origin {
  let c = linux.net_constants()
  let closed = loopback_udp()?
  let dead = linux.getsockname(closed)?
  unix.close_fd(closed)?

  let probe = linux.socket(c.AF_INET, c.SOCK_DGRAM)?
  defer unix.close_fd(probe)
  linux.setsockopt_int(probe, c.SOL_IP, c.IP_RECVERR, 1)?
  linux.connect(probe, dead)?
  assert linux.sendto(probe, bytes.from_text("x"))? == 1
  assert "error" in unix.poll_fd(probe, [], 1000)?

  let queued = linux.recvfrom(probe, 64, c.MSG_ERRQUEUE)?
  assert queued.data == bytes.from_text("x")
  assert queued.flags.bit_and(c.MSG_ERRQUEUE) != 0
  let errors = [m for m in queued.control if m.level == c.SOL_IP and m.type == c.IP_RECVERR]
  assert errors.len() == 1
  let extended = errors[0].data
  assert bytes.unpack_le(extended, 4)? == 111
  assert extended.byte_at(4) == c.SO_EE_ORIGIN_ICMP
  assert extended.byte_at(5) == 3 and extended.byte_at(6) == 3
}

test test_icmp_echo_over_a_datagram_socket {
  let c = linux.net_constants()
  match linux.socket(c.AF_INET, c.SOCK_DGRAM, c.IPPROTO_ICMP) {
    Err(failure) => {
      assert failure.errno == 13 or failure.errno == 93
      test.skip("ICMP datagram sockets are not permitted for this group")
    }
    Ok(fd) => {
      defer unix.close_fd(fd)
      linux.set_socket_timeout(fd, c.SO_RCVTIMEO, 2000)?
      # type 8 (echo request), code 0, checksum and identifier filled by the kernel
      let request = bytes.concat(
        [bytes.from_ints([8, 0, 0, 0, 0, 0, 0, 1])?, bytes.from_text("xsh-echo")],
      )
      let _ = linux.sendto(fd, request, {family: "inet", address: "127.0.0.1"})?
      let reply = linux.recvfrom(fd, 128)?
      assert reply.address.address == "127.0.0.1"
      assert reply.data.byte_at(0) == 0 and reply.data.byte_at(1) == 0
      assert bytes.unpack_be(reply.data, 2, 6)? == 1
      assert reply.data.slice(8) == bytes.from_text("xsh-echo")
    }
  }
}

# The RFC 1071 checksum of an ICMP message with a zeroed checksum field.
pure icmp_checksum(message: Bytes) -> Int {
  var sum = 0
  var offset = 0
  while offset < message.len() {
    let high = message.byte_at(offset) ?? 0
    let low = message.byte_at(offset + 1) ?? 0
    sum += high * 256 + low
    offset += 2
  }
  while sum > 65535 {
    sum = sum.bit_and(65535) + sum / 65536
  }
  65535 - sum
}

test test_raw_icmp_socket_filters_and_checks_replies {
  let c = linux.net_constants()
  match linux.socket(c.AF_INET, c.SOCK_RAW, c.IPPROTO_ICMP) {
    Err(failure) => {
      assert failure.errno == 1 or failure.errno == 13
      test.skip("raw sockets need CAP_NET_RAW")
    }
    Ok(fd) => {
      defer unix.close_fd(fd)
      # struct icmp_filter: a set bit blocks that type, so only echo replies pass.
      let mask = bytes.pack_le(4294967294, 4)?
      linux.setsockopt_bytes(fd, c.SOL_RAW, c.ICMP_FILTER, mask)?
      assert linux.getsockopt_bytes(fd, c.SOL_RAW, c.ICMP_FILTER, 4)? == mask
      linux.set_socket_timeout(fd, c.SO_RCVTIMEO, 2000)?

      let body = bytes.concat([bytes.from_ints([8, 0, 0, 0, 18, 52, 0, 7])?, bytes.from_text("raw")])
      let sum = icmp_checksum(body)
      let request = bytes.concat([body.slice(0, 2), bytes.pack_be(sum, 2)?, body.slice(4)])
      let _ = linux.sendto(fd, request, {family: "inet", address: "127.0.0.1"})?
      let reply = linux.recvfrom(fd, 256)?
      # Raw sockets deliver the IP header; its length is the low nibble in words.
      let header = (reply.data.byte_at(0) ?? 0).bit_and(15) * 4
      assert header >= 20
      assert reply.data.byte_at(header) == 0
      assert bytes.unpack_be(reply.data, 2, header + 4)? == 4660
      assert bytes.unpack_be(reply.data, 2, header + 6)? == 7
      assert reply.data.slice(header + 8) == bytes.from_text("raw")
      assert icmp_checksum(reply.data.slice(header)) == 0
    }
  }
}

test test_socket_options_move_bytes_and_timeouts {
  let c = linux.net_constants()
  let fd = loopback_udp()?
  defer unix.close_fd(fd)

  # struct linger { int l_onoff; int l_linger }
  let linger = bytes.concat([bytes.pack_le(1, 4)?, bytes.pack_le(5, 4)?])
  linux.setsockopt_bytes(fd, c.SOL_SOCKET, c.SO_LINGER, linger)?
  assert linux.getsockopt_bytes(fd, c.SOL_SOCKET, c.SO_LINGER, 8)? == linger
  assert linux.getsockopt_bytes(fd, c.SOL_SOCKET, c.SO_LINGER, 64)?.len() == 8

  linux.set_socket_timeout(fd, c.SO_RCVTIMEO, 60)?
  let timeout = linux.getsockopt_bytes(fd, c.SOL_SOCKET, c.SO_RCVTIMEO, 16)?
  assert bytes.unpack_le(timeout, 8, 0)? == 0
  assert bytes.unpack_le(timeout, 8, 8)? == 60000
  assert errno_of(linux.recvfrom(fd, 16)) == 11

  test.error_kind(linux.set_socket_timeout(fd, c.SO_REUSEADDR, 10), "invalid-argument")
  test.error_kind(linux.set_socket_timeout(fd, c.SO_RCVTIMEO, -1), "invalid-argument")
  test.error_kind(linux.getsockopt_bytes(fd, c.SOL_SOCKET, c.SO_LINGER, 0), "invalid-argument")
  assert errno_of(linux.getsockopt_int(fd, c.SOL_SOCKET, 9999)) == 92
}

test test_failures_carry_the_kernel_errno {
  let c = linux.net_constants()
  let closed = loopback_udp()?
  let dead = linux.getsockname(closed)?
  unix.close_fd(closed)?

  let tcp = linux.socket(c.AF_INET, c.SOCK_STREAM)?
  assert errno_of(linux.connect(tcp, dead)) == 111
  assert errno_of(linux.getpeername(tcp)) == 107
  unix.close_fd(tcp)?
  # A descriptor number no process of this size holds open: EBADF, not a
  # reused number a closed socket might have left behind.
  assert errno_of(linux.getsockname(99999)) == 9
  assert errno_of(linux.setsockopt_int(99999, c.SOL_SOCKET, c.SO_REUSEADDR, 1)) == 9
  assert errno_of(linux.accept(99999)) == 9
  assert errno_of(linux.socket(9999, c.SOCK_STREAM)) == 97
  test.error_kind(linux.recvfrom(-1, 16), "invalid-argument")

  let datagram = loopback_udp()?
  defer unix.close_fd(datagram)
  test.error_kind(linux.bind(datagram, {family: "inet", address: "not-an-ip"}), "invalid-argument")
  test.error_kind(linux.bind(datagram, {family: "carrier-pigeon", address: ""}), "invalid-argument")
  test.error_kind(linux.bind(datagram, {family: "inet", address: "127.0.0.1", port: 70000}), "invalid-argument")
  test.error_kind(linux.bind(datagram, b"\x02"), "invalid-argument")
  test.error_kind(linux.recvfrom(datagram, 0), "invalid-argument")
  test.error_kind(linux.recvfrom(datagram, 8, 1099511627776), "invalid-argument")
}

test test_unix_sockets_use_paths_and_abstract_names {
  let c = linux.net_constants()
  let pid = process.current_pid()?
  # sun_path holds 107 bytes, which a test's scratch directory can exceed.
  let socket_path = f"{env.get_or("TMPDIR", "/tmp") ?? "/tmp"}/xsh-unix-{pid}"
  var names = [f"@xsh-linux-net-{pid}"]
  if socket_path.byte_len() < 100 {
    names += [socket_path]
  }
  for name in names {
    let server = linux.socket(c.AF_UNIX, c.SOCK_STREAM)?
    defer unix.close_fd(server)
    linux.bind(server, {family: "unix", address: name})?
    defer fp"{socket_path}".remove()
    linux.listen(server)?
    let bound = linux.getsockname(server)?
    assert bound.family == "unix" and bound.address == name

    let client = linux.socket(c.AF_UNIX, c.SOCK_STREAM)?
    defer unix.close_fd(client)
    linux.connect(client, bound)?
    let accepted = linux.accept(server, nonblock: true)?
    defer unix.close_fd(accepted.fd)
    assert unix.write_fd(client, bytes.from_text("over unix"))? == 9
    assert unix.read_fd(accepted.fd, 32)? == bytes.from_text("over unix")
    assert accepted.peer.family == "unix"
  }
  test.error_kind(linux.bind(0, {family: "unix", address: ["x" for _ in range(200)].join("")}), "invalid-argument")
}

test test_netlink_link_dump_matches_network_dump {
  let c = linux.net_constants()
  let nl = linux.netlink_open(c.NETLINK_ROUTE)?
  defer unix.close_fd(nl)
  let dump_flags = c.NLM_F_REQUEST.bit_or(c.NLM_F_DUMP)
  let replies = linux.netlink_request(nl, c.RTM_GETLINK, dump_flags, bytes.zero(16)?)?
  assert replies.len() >= 1
  assert [r for r in replies if r.type == c.RTM_NEWLINK].len() == replies.len()
  assert replies[0].flags.bit_and(c.NLM_F_MULTI) != 0
  assert replies[0].seq > 0 and replies[0].pid != 0

  let names = interface_names(replies)?
  assert "lo" in names
  let known = [link.name ?? "" for link in settled_dump()?.links]
  assert names.len() == known.len()
  for name in known {
    assert name in names
  }
}

test test_netlink_address_and_route_dumps_match_network_dump {
  let c = linux.net_constants()
  let nl = linux.netlink_open(c.NETLINK_ROUTE)?
  defer unix.close_fd(nl)
  let dump_flags = c.NLM_F_REQUEST.bit_or(c.NLM_F_DUMP)
  let dump = settled_dump()?

  # struct ifaddrmsg is 8 bytes and struct rtmsg 12; zero asks for every family.
  let addresses = linux.netlink_request(nl, c.RTM_GETADDR, dump_flags, bytes.zero(8)?)?
  assert [a for a in addresses if a.type == c.RTM_NEWADDR].len() == addresses.len()
  assert addresses.len() == dump.addresses.len()
  let lengths = [a.payload.byte_at(1) ?? -1 for a in addresses]
  for address in dump.addresses {
    assert address.prefix_length in lengths
  }

  let routes = linux.netlink_request(nl, c.RTM_GETROUTE, dump_flags, bytes.zero(12)?)?
  assert [r for r in routes if r.type == c.RTM_NEWROUTE].len() == routes.len()
  assert routes.len() == dump.routes.len()
}

test test_netlink_error_replies_become_errno {
  let c = linux.net_constants()
  let nl = linux.netlink_open(c.NETLINK_ROUTE)?
  defer unix.close_fd(nl)
  # ifinfomsg for an interface index that does not exist.
  let missing = bytes.concat(
    [bytes.zero(4)?, bytes.pack_le(999999, 4)?, bytes.zero(8)?],
  )
  let failed = linux.netlink_request(nl, c.RTM_GETLINK, c.NLM_F_REQUEST, missing)
  assert errno_of(failed) == 19
  test.error_kind(failed, "linux-netlink-request")

  # The socket stays usable after a refused request.
  let ok = linux.netlink_request(nl, c.RTM_GETLINK, c.NLM_F_REQUEST.bit_or(c.NLM_F_DUMP), bytes.zero(16)?)?
  assert ok.len() >= 1
  test.error_kind(linux.netlink_request(nl, 70000, 1, b""), "invalid-argument")
  assert errno_of(linux.netlink_open(9999)) == 93
}

test test_netlink_acknowledgement_ends_a_request {
  let c = linux.net_constants()
  let nl = linux.netlink_open(c.NETLINK_ROUTE)?
  defer unix.close_fd(nl)
  # A lookup with NLM_F_ACK returns its data and then waits for the ack, so
  # no unread acknowledgement stays behind for the next request.
  let loopback = bytes.concat([bytes.zero(4)?, bytes.pack_le(1, 4)?, bytes.zero(8)?])
  let flags = c.NLM_F_REQUEST.bit_or(c.NLM_F_ACK)
  let first = linux.netlink_request(nl, c.RTM_GETLINK, flags, loopback)?
  assert first.len() == 1
  assert interface_names(first)? == ["lo"]
  let second = linux.netlink_request(nl, c.RTM_GETLINK, flags, loopback)?
  assert second.len() == 1
  assert second[0].seq != first[0].seq
}

test test_sock_diag_finds_a_tcp_listener {
  let c = linux.net_constants()
  let listener = linux.socket(c.AF_INET, c.SOCK_STREAM)?
  defer unix.close_fd(listener)
  linux.bind(listener, {family: "inet", address: "127.0.0.1", port: 0})?
  linux.listen(listener)?
  let port = linux.getsockname(listener)?.port

  let nl = linux.netlink_open(c.NETLINK_SOCK_DIAG)?
  defer unix.close_fd(nl)
  # struct inet_diag_req_v2: family, protocol, ext, pad, states, then a
  # struct inet_diag_sockid whose cookie of all ones means "any socket".
  let request = bytes.concat(
    [
      bytes.from_ints([c.AF_INET, c.IPPROTO_TCP, 0, 0])?,
      bytes.pack_le(1024, 4)?, # 1 << TCP_LISTEN
      bytes.zero(40)?,
      bytes.from_ints([255, 255, 255, 255, 255, 255, 255, 255])?,
    ],
  )
  let replies = linux.netlink_request(
    nl,
    c.SOCK_DIAG_BY_FAMILY,
    c.NLM_F_REQUEST.bit_or(c.NLM_F_DUMP),
    request,
  )?
  let ours = [r for r in replies if bytes.unpack_be(r.payload, 2, 4)? == port]
  assert ours.len() == 1
  let message = ours[0].payload
  assert message.byte_at(0) == c.AF_INET
  assert message.byte_at(1) == c.TCP_LISTEN
  assert message.slice(8, 4) == bytes.from_ints([127, 0, 0, 1])?
}

test test_generic_netlink_resolves_family_ids {
  let c = linux.net_constants()
  assert linux.genl_family_id("nlctrl")? == 16
  assert errno_of(linux.genl_family_id("xsh-no-such")) == 2
  test.error_kind(linux.genl_family_id(""), "invalid-argument")
  test.error_kind(linux.genl_family_id("a-name-longer-than-fifteen"), "invalid-argument")

  # The same id answers a CTRL_CMD_GETFAMILY on a socket the script owns.
  let nl = linux.netlink_open(c.NETLINK_GENERIC)?
  defer unix.close_fd(nl)
  let name = bytes.concat([bytes.from_text("nlctrl"), bytes.zero(1)?])
  let attribute = bytes.concat(
    [bytes.pack_le(4 + name.len(), 2)?, bytes.pack_le(c.CTRL_ATTR_FAMILY_NAME, 2)?, name, bytes.zero(1)?],
  )
  let payload = bytes.concat([bytes.from_ints([c.CTRL_CMD_GETFAMILY, 1, 0, 0])?, attribute])
  let replies = linux.netlink_request(nl, c.GENL_ID_CTRL, c.NLM_F_REQUEST, payload)?
  assert replies.len() == 1
  let ids = [a for a in attributes(replies[0].payload, 4)? if a.kind == c.CTRL_ATTR_FAMILY_ID]
  assert bytes.unpack_le(ids[0].data, 2)? == 16
}

test test_ioctl_reads_loopback_interface_state {
  let c = linux.net_constants()
  let fd = linux.socket(c.AF_INET, c.SOCK_DGRAM)?
  defer unix.close_fd(fd)

  let flags = linux.ioctl(fd, c.SIOCGIFFLAGS, ifname("lo")?, 40)?
  assert flags.len() == 40
  assert flags.slice(0, 2) == bytes.from_text("lo")
  let word = bytes.unpack_le(flags, 2, 16)?
  assert word.bit_and(c.IFF_UP) != 0
  assert word.bit_and(c.IFF_LOOPBACK) != 0

  let hardware = linux.ioctl(fd, c.SIOCGIFHWADDR, ifname("lo")?, 24)?
  assert bytes.unpack_le(hardware, 2, 16)? == c.ARPHRD_LOOPBACK
  assert hardware.slice(18, 6) == bytes.zero(6)?

  let index = linux.ioctl(fd, c.SIOCGIFINDEX, ifname("lo")?, 20)?
  assert bytes.unpack_le(index, 4, 16)? == 1
  let mtu = linux.ioctl(fd, c.SIOCGIFMTU, ifname("lo")?, 20)?
  assert bytes.unpack_le(mtu, 4, 16)? > 0

  let address = linux.ioctl(fd, c.SIOCGIFADDR, ifname("lo")?, 40)?
  assert bytes.unpack_le(address, 2, 16)? == c.AF_INET
  assert address.slice(20, 4) == bytes.from_ints([127, 0, 0, 1])?
  assert errno_of(linux.ioctl(fd, c.SIOCGIFFLAGS, ifname("xsh-no-such0")?, 40)) == 19
}

test test_ioctl_ethtool_passes_a_payload_pointer {
  let c = linux.net_constants()
  let fd = linux.socket(c.AF_INET, c.SOCK_DGRAM)?
  defer unix.close_fd(fd)
  # ETHTOOL_GLINK: struct ethtool_value { cmd, data } returns link state.
  let glink = bytes.concat([bytes.pack_le(c.ETHTOOL_GLINK, 4)?, bytes.zero(4)?])
  let input = bytes.concat([ifname("lo")?, glink])
  let answer = linux.ioctl(fd, c.SIOCETHTOOL, input, 8)?
  assert bytes.unpack_le(answer, 4, 0)? == c.ETHTOOL_GLINK
  assert bytes.unpack_le(answer, 4, 4)? == 1
}

test test_ioctl_is_guarded {
  let c = linux.net_constants()
  let fd = linux.socket(c.AF_INET, c.SOCK_DGRAM)?
  defer unix.close_fd(fd)
  # TCGETS is a real ioctl, but not one of the admitted structures.
  test.error_kind(linux.ioctl(fd, 21505, bytes.zero(40)?, 40), "invalid-argument")
  test.error_kind(linux.ioctl(fd, c.SIOCGIFFLAGS, bytes.zero(41)?, 40), "invalid-argument")
  test.error_kind(linux.ioctl(fd, c.SIOCGIFFLAGS, ifname("lo")?, 41), "invalid-argument")
  test.error_kind(linux.ioctl(fd, c.SIOCGIFFLAGS, b"lo", -1), "invalid-argument")
  test.error_kind(linux.ioctl(fd, c.SIOCETHTOOL, ifname("lo")?, 0), "invalid-argument")
  test.error_kind(linux.ioctl(-1, c.SIOCGIFFLAGS, b"lo", 40), "invalid-argument")

  let devnull = unix.open_fd(/dev/null)?
  defer unix.close_fd(devnull)
  assert errno_of(linux.ioctl(devnull, c.SIOCGIFFLAGS, ifname("lo")?, 40)) == 25
}

# Reports the one open_files row of descriptor `fd`.
proc descriptor_row(pid: Int, fd: Int) [process, error] -> Result[LinuxOpenFile] {
  let rows = linux.open_files(pid)? |> where .fd == fd |> collect
  assert rows.len() == 1, f"descriptor {fd} must have exactly one row"
  Ok(rows[0])
}

test test_open_files_describes_each_socket_and_a_locked_file_by_its_own_type { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("linux.open_files reads live Linux process descriptors")
    return
  }
  let c = linux.net_constants()
  let pid = process.current_pid()?

  # A connection that closed on this side first leaves a TIME_WAIT row with
  # inode 0 in the TCP table; no descriptor may be matched to it.
  let listener = linux.socket(c.AF_INET, c.SOCK_STREAM)?
  defer unix.close_fd(listener)
  linux.bind(listener, {family: "inet", address: "127.0.0.1", port: 0})?
  linux.listen(listener, 4)?
  let tcp_address = linux.getsockname(listener)?
  let first = linux.socket(c.AF_INET, c.SOCK_STREAM)?
  linux.connect(first, tcp_address)?
  let first_peer = linux.accept(listener)?
  unix.close_fd(first_peer.fd)?
  unix.close_fd(first)?

  let client = linux.socket(c.AF_INET, c.SOCK_STREAM)?
  defer unix.close_fd(client)
  linux.connect(client, tcp_address)?
  let accepted = linux.accept(listener)?
  defer unix.close_fd(accepted.fd)
  let client_address = linux.getsockname(client)?

  let datagram = loopback_udp()?
  defer unix.close_fd(datagram)
  let datagram_address = linux.getsockname(datagram)?

  let unix_name = f"@xsh-open-files-{pid}"
  let local_server = linux.socket(c.AF_UNIX, c.SOCK_STREAM)?
  defer unix.close_fd(local_server)
  linux.bind(local_server, {family: "unix", address: unix_name})?
  linux.listen(local_server)?

  let netlink = linux.netlink_open(c.NETLINK_ROUTE)?
  defer unix.close_fd(netlink)

  let root = test.temp_dir(ctx, name: "open-files-lock")?
  let locked = fp"{root}/locked.txt"
  locked.write("held")
  let lock = fs.lock(locked)?
  defer fs.unlock(lock)

  let listener_row = descriptor_row(pid, listener)?
  assert listener_row.type == "socket" and listener_row.protocol == "tcp"
  assert listener_row.local == f"127.0.0.1:{tcp_address.port}"
  assert listener_row.remote == "0.0.0.0:0"

  let client_row = descriptor_row(pid, client)?
  assert client_row.type == "socket" and client_row.protocol == "tcp"
  assert client_row.local == f"127.0.0.1:{client_address.port}"
  assert client_row.remote == f"127.0.0.1:{tcp_address.port}"

  let accepted_row = descriptor_row(pid, accepted.fd)?
  assert accepted_row.type == "socket" and accepted_row.protocol == "tcp"
  assert accepted_row.local == f"127.0.0.1:{tcp_address.port}"
  assert accepted_row.remote == f"127.0.0.1:{client_address.port}"
  assert listener_row.inode != client_row.inode
  assert client_row.inode != accepted_row.inode

  let datagram_row = descriptor_row(pid, datagram)?
  assert datagram_row.type == "socket" and datagram_row.protocol == "udp"
  assert datagram_row.local == f"127.0.0.1:{datagram_address.port}"

  let unix_row = descriptor_row(pid, local_server)?
  assert unix_row.type == "socket" and unix_row.protocol == "unix"
  assert unix_row.local == unix_name and unix_row.remote == ""

  let netlink_row = descriptor_row(pid, netlink)?
  assert netlink_row.type == "socket" and netlink_row.protocol == "netlink"

  let files = linux.open_files(pid)? |> where .path == locked |> collect
  assert files.len() == 1, "the locked regular file must have one row"
  let locked_row = files[0]
  assert locked_row.type == "file"
  assert locked_row.protocol == "" and locked_row.local == "" and locked_row.remote == ""
  assert locked_row.inode == fs.stat(locked)?.ino

  # No descriptor other than a socket may carry socket attributes.
  for row in linux.open_files(pid)? {
    if row.type != "socket" {
      assert row.protocol == "" and row.local == "" and row.remote == "", f"fd {row.fd} is {row.type}"
    }
  }
}
