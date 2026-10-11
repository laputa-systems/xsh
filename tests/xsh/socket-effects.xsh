# Bodies remain uncalled so checker probes do not depend on Linux, privilege,
# live sockets, or the descriptor numbers supplied to host operations.
const socket_calls = [
  "linux.socket(2, 1)",
  "linux.connect(0, {family: \"inet\", address: \"127.0.0.1\", port: 80})",
  "linux.bind(0, {family: \"inet\", address: \"127.0.0.1\", port: 80})",
  "linux.listen(0)",
  "linux.accept(0)",
  "linux.sendto(0, b\"payload\")",
  "linux.recvfrom(0, 16)",
  "linux.dhcp_socket(\"eth0\")",
  "linux.dhcp_send(0, b\"payload\")",
  "linux.dhcp_recv(0, 100)",
  "linux.dhcp_send_release(\"eth0\", \"192.0.2.1\", \"192.0.2.2\")",
  "unix.read_fd(0, 16)",
  "unix.write_fd(0, b\"payload\")",
  "unix.notify_ready(0)",
  "linux.link_up(\"eth0\")",
  "linux.link_down(\"eth0\")",
  "linux.set_ipv4_address(\"eth0\", \"192.0.2.1\", \"255.255.255.0\")",
  "linux.flush_ipv4_addresses(\"eth0\")",
  "linux.add_default_ipv4_route(\"192.0.2.2\")",
  "linux.del_default_ipv4_route(\"192.0.2.2\", \"eth0\")",
]

proc check_source(ctx: TestContext, source: Str, status: Int, required = "net") [fs, process, env, error] {
  let result = test.run_script(ctx, source)?
  assert result.status == status, f"{source}\n{result.stdout}{result.stderr}"
  if status == 2 {
    assert "err[check.effect-violation]" in result.stderr, result.stderr
    assert f"`{required}`" in result.stderr, result.stderr
  } else {
    assert result.stderr == "", result.stderr
  }
}

test test_socket_calls_require_both_process_and_net { |ctx|
  for call in socket_calls {
    check_source(ctx, f"proc probe() [process] {{ let _ = {call} }}", 2)
    check_source(ctx, f"proc probe() [net] {{ let _ = {call} }}", 2, required: "process")
    check_source(ctx, f"proc probe() [process, net] {{ let _ = {call} }}", 0)
    check_source(ctx, f"proc probe() [io] {{ let _ = {call} }}", 0)
  }
}

test test_without_net_excludes_socket_calls { |ctx|
  for call in socket_calls {
    check_source(ctx, f"proc probe() [process, net] {{ without net {{ let _ = {call} }} }}", 2)
    check_source(ctx, f"proc probe() [process, net] {{ without fs {{ let _ = {call} }} }}", 0)
    check_source(ctx, f"proc probe() [process, net] {{ without process {{ let _ = {call} }} }}", 2, required: "process")
  }
}

test test_inferred_socket_callees_publish_both_effects { |ctx|
  for call in [socket_calls[0], socket_calls[1], socket_calls[11]] {
    check_source(
      ctx,
      f"proc operation() {{ let _ = {call} }}\nproc probe() [process, error] {{ operation()? }}",
      2,
    )
  }
}

test test_socket_commands_require_net { |ctx|
  for call in [
    "linux.connect 0 ({family: \"inet\", address: \"127.0.0.1\", port: 80})",
    "linux.bind 0 ({family: \"inet\", address: \"127.0.0.1\", port: 80})",
    "linux.link_up \"eth0\"",
    "linux.dhcp_send 0 (b\"payload\")",
  ] {
    check_source(ctx, f"proc probe() [process, error] {{ {call} }}", 2)
    check_source(ctx, f"proc probe() [process, net, error] {{ {call} }}", 0)
  }
}

test test_dedicated_netlink_stays_process_only { |ctx|
  check_source(ctx, r"""proc probe() [process] {
  without net {
    let _ = linux.netlink_open(0)
    let _ = linux.netlink_request(0, 16, 1, b"")
    let _ = linux.genl_family_id("nlctrl")
    let _ = linux.network_dump()
  }
}
""", 0)
}

test test_raw_families_do_not_make_generic_socket_calls_local { |ctx|
  for call in [
    "linux.socket(1, 1)",
    "linux.socket(16, 3)",
    "linux.connect(0, {family: \"unix\", address: \"/tmp/service\"})",
    "linux.recvfrom(0, 16)",
  ] {
    check_source(ctx, f"proc probe() [process] {{ let _ = {call} }}", 2)
  }
}

test test_socket_metadata_and_release_keep_the_process_effect { |ctx|
  check_source(ctx, r"""proc probe() [process] {
  without net {
    let _ = linux.shutdown(0, 0)
    let _ = linux.getsockname(0)
    let _ = linux.getpeername(0)
    let _ = linux.setsockopt_int(0, 0, 0, 0)
    let _ = linux.getsockopt_int(0, 0, 0)
    let _ = linux.setsockopt_bytes(0, 0, 0, b"")
    let _ = linux.getsockopt_bytes(0, 0, 0, 16)
    let _ = linux.set_socket_timeout(0, 0, 1)
    let _ = linux.ioctl(0, 0, b"", 0)
    let _ = linux.dhcp_close(0)
    let _ = unix.close_fd(0)
    let _ = unix.notify_close(0)
    let _ = unix.poll_fd(0, [])
  }
}
""", 0)
}

test test_external_programs_keep_their_process_contract { |ctx|
  check_source(ctx, r"""proc probe() [process] {
  without net {
    let _ = run.status curl https://example.test/
  }
}
""", 0)
}
