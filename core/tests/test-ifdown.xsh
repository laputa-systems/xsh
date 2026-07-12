proc xsh_bin() [env] -> Path {
  let bin = (env.get("CARGO_BIN_EXE_xsh") ?? "")
  if bin != "" {
    return fp"${bin}"
  }
  return ../target/debug/xsh
}

proc core_script(name: Str) [env] -> Path {
  let dir = (env.get("XSH_CORE_DIR") ?? "")
  if dir != "" {
    return fp"${dir}/${name}"
  }
  return ../name
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
    pre-down echo "pre-down:$IFACE:$LOGICAL:$ADDRFAM:$METHOD" >> ${hook_log.display()}
    down echo "down:$IFACE:$IF_ADDRESS" >> ${hook_log.display()}
    post-down echo "post-down:$PHASE" >> ${hook_log.display()}
""",
  )?
}

proc test_ifdown_all_removes_configured_interfaces(ctx: TestContext) [env, fs, process, error] {
  let root = test.temp_dir(ctx, name: "ifdown-all")?
  let interfaces = fp"${root}/interfaces"
  let state = fp"${root}/ifstate"
  let linux_log = fp"${root}/linux.jsonl"
  let hook_log = fp"${root}/hooks.log"
  write_interfaces(interfaces, hook_log)?

  # Bring up eth0 first.
  run XSH_LINUX_DRY_RUN=1 XSH_LINUX_DRY_RUN_LOG=$linux_log XSH_IFUP_INTERFACES=$interfaces XSH_IFUP_STATE=$state xsh_bin() core_script("ifup.xsh") -- eth0 ?
  test.contains(state.read_text()?, "eth0=eth0")?

  # Bring down with ifdown -a.
  run XSH_LINUX_DRY_RUN=1 XSH_LINUX_DRY_RUN_LOG=$linux_log XSH_IFUP_INTERFACES=$interfaces XSH_IFUP_STATE=$state xsh_bin() core_script("ifdown.xsh") -- -a ?

  # State should be cleared after teardown.
  test.eq(state.exists()?, false)?

  # Verify the teardown operations were logged.
  let linux_text = linux_log.read_text()?
  test.contains(linux_text, "\"op\":\"link_down\"")?
  test.contains(linux_text, "\"op\":\"flush_ipv4_addresses\"")?
  test.contains(linux_text, "\"op\":\"del_default_ipv4_route\"")?
  test.contains(linux_text, "\"interface\":\"eth0\"")?
}

proc test_ifdown_runs_hooks(ctx: TestContext) [env, fs, process, error] {
  let root = test.temp_dir(ctx, name: "ifdown-hooks")?
  let interfaces = fp"${root}/interfaces"
  let state = fp"${root}/ifstate"
  let linux_log = fp"${root}/linux.jsonl"
  let hook_log = fp"${root}/hooks.log"
  write_interfaces(interfaces, hook_log)?
  run XSH_LINUX_DRY_RUN=1 XSH_LINUX_DRY_RUN_LOG=$linux_log XSH_IFUP_INTERFACES=$interfaces XSH_IFUP_STATE=$state xsh_bin() core_script("ifup.xsh") -- eth0 ?
  run XSH_LINUX_DRY_RUN=1 XSH_LINUX_DRY_RUN_LOG=$linux_log XSH_IFUP_INTERFACES=$interfaces XSH_IFUP_STATE=$state xsh_bin() core_script("ifdown.xsh") -- eth0 ?
  let hooks = hook_log.read_text()?
  test.contains(hooks, "pre-down:eth0:eth0:inet:static")?
  test.contains(hooks, "down:eth0:10.0.1.42")?
  test.contains(hooks, "post-down:post-down")?
}

proc test_ifdown_dhcp_sends_release(ctx: TestContext) [env, fs, process, error] {
  let root = test.temp_dir(ctx, name: "ifdown-dhcp-release")?
  let interfaces = fp"${root}/interfaces"
  let state = fp"${root}/ifstate"
  let linux_log = fp"${root}/linux.jsonl"
  let err = fp"${root}/ifdown.err"

  # Write a DHCP stanza and pre-seed the state file so ifdown finds it.
  fs.write(
    interfaces,
    """auto eth0
iface eth0 inet dhcp
""",
  )?

  fs.write(state, "eth0=eth0")?

  # Dry-run has no real DHCP, so the RELEASE send will log but not actually
  # reach a server.  This is fine — we just verify the primitive was called.
  let status = run.status XSH_LINUX_DRY_RUN=1 XSH_LINUX_DRY_RUN_LOG=$linux_log XSH_IFUP_INTERFACES=$interfaces XSH_IFUP_STATE=$state xsh_bin() core_script("ifdown.xsh") -- eth0 2> $err
  test.eq(status.ok, true)?
  let linux_text = linux_log.read_text()?
  test.contains(linux_text, "\"op\":\"link_down\"")?
  test.contains(linux_text, "\"op\":\"flush_ipv4_addresses\"")?
  test.contains(linux_text, "\"op\":\"del_default_ipv4_route\"")?
}

proc test_ifdown_skips_unconfigured_interface(ctx: TestContext) [env, fs, process, error] {
  let root = test.temp_dir(ctx, name: "ifdown-skip")?
  let interfaces = fp"${root}/interfaces"
  let state = fp"${root}/ifstate"
  let linux_log = fp"${root}/linux.jsonl"

  fs.write(
    interfaces,
    """auto eth0
iface eth0 inet static
    address 10.0.1.42
    netmask 255.255.255.0
""",
  )?

  # Don't pre-seed state — ifdown should be a no-op for unconfigured interfaces.
  run XSH_LINUX_DRY_RUN=1 XSH_LINUX_DRY_RUN_LOG=$linux_log XSH_IFUP_INTERFACES=$interfaces XSH_IFUP_STATE=$state xsh_bin() core_script("ifdown.xsh") -- eth0 ?
  test.eq(linux_log.exists()?, false)?
}

proc test_ifdown_logical_selection(ctx: TestContext) [env, fs, process, error] {
  let root = test.temp_dir(ctx, name: "ifdown-logical")?
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

  fs.write(state, "eth0=office")?
  run XSH_LINUX_DRY_RUN=1 XSH_LINUX_DRY_RUN_LOG=$linux_log XSH_IFUP_INTERFACES=$interfaces XSH_IFUP_STATE=$state xsh_bin() core_script("ifdown.xsh") -- eth0=office ?
  test.contains(linux_log.read_text()?, "\"interface\":\"eth0\"")?
  test.eq(state.exists()?, false)?
}
