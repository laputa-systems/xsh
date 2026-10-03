type AppletRun = {success: Bool, status: Int, stdout: Str, stderr: Str, stdout_bytes: Bytes, stderr_bytes: Bytes}

# Runs a core applet under the `linux` test fake, which logs each linux.* call
# to `linux_log` instead of changing the host's network.
proc run_ifupdown(
  ctx: TestContext,
  name: Str,
  argv: List[Str],
  interfaces: Path,
  state: Path,
  linux_log: Path,
) [fs, process, error] -> AppletRun {
  test.linux_fake(ctx, {log: linux_log})?
  let source = fp"${ctx.core_dir}/${name}.xsh".read_text()?
  let overlay = {XSH_IFUP_INTERFACES: interfaces.display(), XSH_IFUP_STATE: state.display()}
  test.run_script(ctx, source, argv, overlay, b"", name)?
}

proc ifupdown_ok(
  ctx: TestContext,
  name: Str,
  argv: List[Str],
  interfaces: Path,
  state: Path,
  linux_log: Path,
) [fs, process, error] {
  let output = run_ifupdown(ctx, name, argv, interfaces, state, linux_log)
  assert output.success, output.stderr
}

proc write_interfaces(path_value: Path, hook_log: Path) [fs, error] {
  fs.write(
    path_value,
    f"""auto lo eth0
iface lo inet loopback

iface eth0 inet static
    address 10.0.1.42
    netmask 255.255.255.0
    gateway 10.0.1.1
    pre-up echo "pre:\$IFACE:\$LOGICAL:\$ADDRFAM:\$METHOD" >> ${hook_log.display()}
    up echo "up:\$IFACE:\$IF_ADDRESS:\$IF_GATEWAY" >> ${hook_log.display()}
    post-up echo "post:\$PHASE" >> ${hook_log.display()}
""",
  )?
}

test test_ifup_all_applies_auto_static_and_hooks { |ctx|
  let root = test.temp_dir(ctx, name: "ifup-all")?
  let interfaces = fp"${root}/interfaces"
  let state = fp"${root}/ifstate"
  let linux_log = fp"${root}/linux.jsonl"
  let hook_log = fp"${root}/hooks.log"
  write_interfaces(interfaces, hook_log)?

  ifupdown_ok(ctx, "ifup", ["-a"], interfaces, state, linux_log)

  let linux_text = linux_log.read_text()?
  assert "\"op\":\"link_up\"" in linux_text
  assert "\"interface\":\"lo\"" in linux_text
  assert "\"interface\":\"eth0\"" in linux_text
  assert "\"op\":\"set_ipv4_address\"" in linux_text
  assert "\"address\":\"10.0.1.42\"" in linux_text
  assert "\"op\":\"add_default_ipv4_route\"" in linux_text
  assert "\"gateway\":\"10.0.1.1\"" in linux_text
  let hooks = hook_log.read_text()?
  assert "pre:eth0:eth0:inet:static" in hooks
  assert "up:eth0:10.0.1.42:10.0.1.1" in hooks
  assert "post:post-up" in hooks
  assert "eth0=eth0" in state.read_text()?
}

test test_ifup_dhcp_runs_discovery { |ctx|
  let root = test.temp_dir(ctx, name: "ifup-dhcp")?
  let interfaces = fp"${root}/interfaces"
  let state = fp"${root}/ifstate"
  let linux_log = fp"${root}/linux.jsonl"

  fs.write(
    interfaces,
    """auto eth0
iface eth0 inet dhcp
""",
  )?

  let output = run_ifupdown(ctx, "ifup", ["-a"], interfaces, state, linux_log)

  # The fake has no DHCP server, so discovery wires the sockets then fails cleanly.
  assert !output.success
  let linux_text = linux_log.read_text()?
  assert "\"op\":\"link_up\"" in linux_text
  assert "\"op\":\"dhcp_socket\"" in linux_text
  assert "\"interface\":\"eth0\"" in linux_text
  assert "\"op\":\"dhcp_send\"" in linux_text
  assert "\"op\":\"dhcp_recv\"" in linux_text
  assert "\"op\":\"dhcp_close\"" in linux_text
  assert "no DHCP offer" in output.stderr
}

test test_ifup_state_skips_configured_interface { |ctx|
  let root = test.temp_dir(ctx, name: "ifup-state")?
  let interfaces = fp"${root}/interfaces"
  let state = fp"${root}/ifstate"
  let linux_log = fp"${root}/linux.jsonl"
  let hook_log = fp"${root}/hooks.log"
  write_interfaces(interfaces, hook_log)?

  ifupdown_ok(ctx, "ifup", ["eth0"], interfaces, state, linux_log)

  ifupdown_ok(ctx, "ifup", ["eth0"], interfaces, state, linux_log)

  assert hook_log.read_text()?.split("up:eth0").len() == 2
}

test test_ifup_logical_selection { |ctx|
  let root = test.temp_dir(ctx, name: "ifup-logical")?
  let interfaces = fp"${root}/interfaces"
  let state = fp"${root}/ifstate"
  let linux_log = fp"${root}/linux.jsonl"

  fs.write(
    interfaces,
    """iface office inet static
    address 10.0.1.42
    netmask 255.255.255.0
""",
  )?

  ifupdown_ok(ctx, "ifup", ["eth0=office"], interfaces, state, linux_log)

  assert "\"interface\":\"eth0\"" in linux_log.read_text()?
  assert "eth0=office" in state.read_text()?
}

test test_ifup_source_glob { |ctx|
  let root = test.temp_dir(ctx, name: "ifup-source")?
  let interfaces = fp"${root}/interfaces"
  let sourced = fp"${root}/interfaces.d"
  let state = fp"${root}/ifstate"
  let linux_log = fp"${root}/linux.jsonl"
  fs.mkdir(sourced)?

  fs.write(
    interfaces,
    f"""source ${sourced.display()}/*
auto eth0
""",
  )?

  fs.write(
    fp"${sourced}/eth0",
    """iface eth0 inet static
    address 10.0.1.42
    netmask 255.255.255.0
""",
  )?

  ifupdown_ok(ctx, "ifup", ["-a"], interfaces, state, linux_log)

  assert "\"address\":\"10.0.1.42\"" in linux_log.read_text()?
}
