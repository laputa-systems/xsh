proc write_interfaces(path_value: Path, hook_log: Path) [fs, error] {
  fs.write(
    path_value,
    f"""auto lo eth0
iface lo inet loopback

iface eth0 inet static
    address 10.0.1.42
    netmask 255.255.255.0
    gateway 10.0.1.1
    pre-up echo "pre:$IFACE:$LOGICAL:$ADDRFAM:$METHOD" >> ${hook_log.display()}
    up echo "up:$IFACE:$IF_ADDRESS:$IF_GATEWAY" >> ${hook_log.display()}
    post-up echo "post:$PHASE" >> ${hook_log.display()}
""",
  )?
}

proc test_ifup_all_applies_auto_static_and_hooks(ctx: TestContext) [fs, process, env, error] {
  let root = test.temp_dir(ctx, name: "ifup-all")?
  let interfaces = fp"${root}/interfaces"
  let state = fp"${root}/ifstate"
  let linux_log = fp"${root}/linux.jsonl"
  let hook_log = fp"${root}/hooks.log"
  write_interfaces(interfaces, hook_log)?

  run XSH_LINUX_DRY_RUN=1 XSH_LINUX_DRY_RUN_LOG=$linux_log XSH_IFUP_INTERFACES=$interfaces XSH_IFUP_STATE=$state ${ctx.xsh_bin} fp"${ctx.core_dir}/ifup.xsh" -- -a ?

  let linux_text = linux_log.read_text()?
  test.contains(linux_text, "\"op\":\"link_up\"")?
  test.contains(linux_text, "\"interface\":\"lo\"")?
  test.contains(linux_text, "\"interface\":\"eth0\"")?
  test.contains(linux_text, "\"op\":\"set_ipv4_address\"")?
  test.contains(linux_text, "\"address\":\"10.0.1.42\"")?
  test.contains(linux_text, "\"op\":\"add_default_ipv4_route\"")?
  test.contains(linux_text, "\"gateway\":\"10.0.1.1\"")?
  let hooks = hook_log.read_text()?
  test.contains(hooks, "pre:eth0:eth0:inet:static")?
  test.contains(hooks, "up:eth0:10.0.1.42:10.0.1.1")?
  test.contains(hooks, "post:post-up")?
  test.contains(state.read_text()?, "eth0=eth0")?
}

proc test_ifup_dhcp_runs_discovery(ctx: TestContext) [fs, process, env, error] {
  let root = test.temp_dir(ctx, name: "ifup-dhcp")?
  let interfaces = fp"${root}/interfaces"
  let state = fp"${root}/ifstate"
  let linux_log = fp"${root}/linux.jsonl"
  let err = fp"${root}/ifup.err"

  fs.write(
    interfaces,
    """auto eth0
iface eth0 inet dhcp
""",
  )?

  let status = run.status XSH_LINUX_DRY_RUN=1 XSH_LINUX_DRY_RUN_LOG=$linux_log XSH_IFUP_INTERFACES=$interfaces XSH_IFUP_STATE=$state ${ctx.xsh_bin} fp"${ctx.core_dir}/ifup.xsh" -- -a 2> $err

  # Dry-run has no DHCP server, so discovery wires the sockets then fails cleanly.
  test.eq(status.ok, false)?
  let linux_text = linux_log.read_text()?
  test.contains(linux_text, "\"op\":\"link_up\"")?
  test.contains(linux_text, "\"op\":\"dhcp_socket\"")?
  test.contains(linux_text, "\"interface\":\"eth0\"")?
  test.contains(linux_text, "\"op\":\"dhcp_send\"")?
  test.contains(linux_text, "\"op\":\"dhcp_recv\"")?
  test.contains(linux_text, "\"op\":\"dhcp_close\"")?
  test.contains(err.read_text()?, "no DHCP offer")?
}

proc test_ifup_state_skips_configured_interface(ctx: TestContext) [fs, process, env, error] {
  let root = test.temp_dir(ctx, name: "ifup-state")?
  let interfaces = fp"${root}/interfaces"
  let state = fp"${root}/ifstate"
  let linux_log = fp"${root}/linux.jsonl"
  let hook_log = fp"${root}/hooks.log"
  write_interfaces(interfaces, hook_log)?

  run XSH_LINUX_DRY_RUN=1 XSH_LINUX_DRY_RUN_LOG=$linux_log XSH_IFUP_INTERFACES=$interfaces XSH_IFUP_STATE=$state ${ctx.xsh_bin} fp"${ctx.core_dir}/ifup.xsh" -- eth0 ?

  run XSH_LINUX_DRY_RUN=1 XSH_LINUX_DRY_RUN_LOG=$linux_log XSH_IFUP_INTERFACES=$interfaces XSH_IFUP_STATE=$state ${ctx.xsh_bin} fp"${ctx.core_dir}/ifup.xsh" -- eth0 ?

  test.eq(hook_log.read_text()?.split("up:eth0").len(), 2)?
}

proc test_ifup_logical_selection(ctx: TestContext) [fs, process, env, error] {
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

  run XSH_LINUX_DRY_RUN=1 XSH_LINUX_DRY_RUN_LOG=$linux_log XSH_IFUP_INTERFACES=$interfaces XSH_IFUP_STATE=$state ${ctx.xsh_bin} fp"${ctx.core_dir}/ifup.xsh" -- eth0=office ?

  test.contains(linux_log.read_text()?, "\"interface\":\"eth0\"")?
  test.contains(state.read_text()?, "eth0=office")?
}

proc test_ifup_source_glob(ctx: TestContext) [fs, process, env, error] {
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

  run XSH_LINUX_DRY_RUN=1 XSH_LINUX_DRY_RUN_LOG=$linux_log XSH_IFUP_INTERFACES=$interfaces XSH_IFUP_STATE=$state ${ctx.xsh_bin} fp"${ctx.core_dir}/ifup.xsh" -- -a ?

  test.contains(linux_log.read_text()?, "\"address\":\"10.0.1.42\"")?
}
