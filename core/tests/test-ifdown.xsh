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
  test.linux_fake(ctx, {log: linux_log})
  let source = fp"{ctx.core_dir}/{name}.xsh".read_text()?
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
    pre-down echo "pre-down:\$IFACE:\$LOGICAL:\$ADDRFAM:\$METHOD" >> {hook_log}
    down echo "down:\$IFACE:\$IF_ADDRESS" >> {hook_log}
    post-down echo "post-down:\$PHASE" >> {hook_log}
""",
  )
}

test test_ifdown_all_removes_configured_interfaces { |ctx|
  let root = test.temp_dir(ctx, name: "ifdown-all")?
  let interfaces = fp"{root}/interfaces"
  let state = fp"{root}/ifstate"
  let linux_log = fp"{root}/linux.jsonl"
  let hook_log = fp"{root}/hooks.log"
  write_interfaces(interfaces, hook_log)

  # Bring up eth0 first.
  ifupdown_ok(ctx, "ifup", ["eth0"], interfaces, state, linux_log)

  assert "eth0=eth0" in state.read_text()?

  # Bring down with ifdown -a.
  ifupdown_ok(ctx, "ifdown", ["-a"], interfaces, state, linux_log)

  # State should be cleared after teardown.
  assert state.exists()? == false

  # Verify the teardown operations were logged.
  let linux_text = linux_log.read_text()?
  assert "\"op\":\"link_down\"" in linux_text
  assert "\"op\":\"flush_ipv4_addresses\"" in linux_text
  assert "\"op\":\"del_default_ipv4_route\"" in linux_text
  assert "\"interface\":\"eth0\"" in linux_text
}

test test_ifdown_runs_hooks { |ctx|
  let root = test.temp_dir(ctx, name: "ifdown-hooks")?
  let interfaces = fp"{root}/interfaces"
  let state = fp"{root}/ifstate"
  let linux_log = fp"{root}/linux.jsonl"
  let hook_log = fp"{root}/hooks.log"
  write_interfaces(interfaces, hook_log)

  ifupdown_ok(ctx, "ifup", ["eth0"], interfaces, state, linux_log)

  ifupdown_ok(ctx, "ifdown", ["eth0"], interfaces, state, linux_log)

  let hooks = hook_log.read_text()?
  assert "pre-down:eth0:eth0:inet:static" in hooks
  assert "down:eth0:10.0.1.42" in hooks
  assert "post-down:post-down" in hooks
}

test test_ifdown_dhcp_sends_release { |ctx|
  let root = test.temp_dir(ctx, name: "ifdown-dhcp-release")?
  let interfaces = fp"{root}/interfaces"
  let state = fp"{root}/ifstate"
  let linux_log = fp"{root}/linux.jsonl"

  # Write a DHCP stanza and pre-seed the state file so ifdown finds it.
  fs.write(
    interfaces,
    """auto eth0
iface eth0 inet dhcp
""",
  )

  fs.write(state, "eth0=eth0")

  # The fake has no real DHCP, so the RELEASE send will log but not actually
  # reach a server.  This is fine — we just verify the primitive was called.
  let output = run_ifupdown(ctx, "ifdown", ["eth0"], interfaces, state, linux_log)

  assert output.success, output.stderr
  let linux_text = linux_log.read_text()?
  assert "\"op\":\"link_down\"" in linux_text
  assert "\"op\":\"flush_ipv4_addresses\"" in linux_text
  assert "\"op\":\"del_default_ipv4_route\"" in linux_text
}

test test_ifdown_skips_unconfigured_interface { |ctx|
  let root = test.temp_dir(ctx, name: "ifdown-skip")?
  let interfaces = fp"{root}/interfaces"
  let state = fp"{root}/ifstate"
  let linux_log = fp"{root}/linux.jsonl"

  fs.write(
    interfaces,
    """auto eth0
iface eth0 inet static
    address 10.0.1.42
    netmask 255.255.255.0
""",
  )

  # Don't pre-seed state — ifdown should be a no-op for unconfigured interfaces.
  ifupdown_ok(ctx, "ifdown", ["eth0"], interfaces, state, linux_log)

  assert linux_log.exists()? == false
}

test test_ifdown_logical_selection { |ctx|
  let root = test.temp_dir(ctx, name: "ifdown-logical")?
  let interfaces = fp"{root}/interfaces"
  let state = fp"{root}/ifstate"
  let linux_log = fp"{root}/linux.jsonl"

  fs.write(
    interfaces,
    """iface office inet static
    address 10.0.1.42
    netmask 255.255.255.0
""",
  )

  fs.write(state, "eth0=office")

  ifupdown_ok(ctx, "ifdown", ["eth0=office"], interfaces, state, linux_log)

  assert "\"interface\":\"eth0\"" in linux_log.read_text()?
  assert state.exists()? == false
}

test test_ifdown_rejects_unimplemented_verbose_mode { |ctx|
  let root = test.temp_dir(ctx, name: "ifdown-verbose")?
  let interfaces = fp"{root}/interfaces"
  let state = fp"{root}/ifstate"
  let linux_log = fp"{root}/linux.jsonl"

  for option in ["-v", "--verbose"] {
    let output = run_ifupdown(ctx, "ifdown", [option, "eth0"], interfaces, state, linux_log)
    assert ! output.success, option
    assert "is not supported" in output.stderr, output.stderr
  }
}
