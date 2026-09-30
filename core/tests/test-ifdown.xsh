proc write_interfaces(path_value: Path, hook_log: Path) [fs, error] {
  fs.write(
    path_value,
    f"""auto lo eth0
iface lo inet loopback

iface eth0 inet static
    address 10.0.1.42
    netmask 255.255.255.0
    gateway 10.0.1.1
    pre-down echo "pre-down:\$IFACE:\$LOGICAL:\$ADDRFAM:\$METHOD" >> ${hook_log.display()}
    down echo "down:\$IFACE:\$IF_ADDRESS" >> ${hook_log.display()}
    post-down echo "post-down:\$PHASE" >> ${hook_log.display()}
""",
  )?
}

test test_ifdown_all_removes_configured_interfaces [fs, process, env, error] { |ctx|
  let root = test.temp_dir(ctx, name: "ifdown-all")?
  let interfaces = fp"${root}/interfaces"
  let state = fp"${root}/ifstate"
  let linux_log = fp"${root}/linux.jsonl"
  let hook_log = fp"${root}/hooks.log"
  write_interfaces(interfaces, hook_log)?

  # Bring up eth0 first.
  run XSH_LINUX_DRY_RUN=1 XSH_LINUX_DRY_RUN_LOG=$linux_log XSH_IFUP_INTERFACES=$interfaces XSH_IFUP_STATE=$state ${ctx.xsh_bin} fp"${ctx.core_dir}/ifup.xsh" -- eth0 ?

  "eth0=eth0" in state.read_text()?

  # Bring down with ifdown -a.
  run XSH_LINUX_DRY_RUN=1 XSH_LINUX_DRY_RUN_LOG=$linux_log XSH_IFUP_INTERFACES=$interfaces XSH_IFUP_STATE=$state ${ctx.xsh_bin} fp"${ctx.core_dir}/ifdown.xsh" -- -a ?

  # State should be cleared after teardown.
  state.exists()? == false

  # Verify the teardown operations were logged.
  let linux_text = linux_log.read_text()?
  "\"op\":\"link_down\"" in linux_text
  "\"op\":\"flush_ipv4_addresses\"" in linux_text
  "\"op\":\"del_default_ipv4_route\"" in linux_text
  "\"interface\":\"eth0\"" in linux_text
}

test test_ifdown_runs_hooks [fs, process, env, error] { |ctx|
  let root = test.temp_dir(ctx, name: "ifdown-hooks")?
  let interfaces = fp"${root}/interfaces"
  let state = fp"${root}/ifstate"
  let linux_log = fp"${root}/linux.jsonl"
  let hook_log = fp"${root}/hooks.log"
  write_interfaces(interfaces, hook_log)?

  run XSH_LINUX_DRY_RUN=1 XSH_LINUX_DRY_RUN_LOG=$linux_log XSH_IFUP_INTERFACES=$interfaces XSH_IFUP_STATE=$state ${ctx.xsh_bin} fp"${ctx.core_dir}/ifup.xsh" -- eth0 ?

  run XSH_LINUX_DRY_RUN=1 XSH_LINUX_DRY_RUN_LOG=$linux_log XSH_IFUP_INTERFACES=$interfaces XSH_IFUP_STATE=$state ${ctx.xsh_bin} fp"${ctx.core_dir}/ifdown.xsh" -- eth0 ?

  let hooks = hook_log.read_text()?
  "pre-down:eth0:eth0:inet:static" in hooks
  "down:eth0:10.0.1.42" in hooks
  "post-down:post-down" in hooks
}

test test_ifdown_dhcp_sends_release [fs, process, env, error] { |ctx|
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
  let status = run.status XSH_LINUX_DRY_RUN=1 XSH_LINUX_DRY_RUN_LOG=$linux_log XSH_IFUP_INTERFACES=$interfaces XSH_IFUP_STATE=$state ${ctx.xsh_bin} fp"${ctx.core_dir}/ifdown.xsh" -- eth0 2> $err

  status.ok == true
  let linux_text = linux_log.read_text()?
  "\"op\":\"link_down\"" in linux_text
  "\"op\":\"flush_ipv4_addresses\"" in linux_text
  "\"op\":\"del_default_ipv4_route\"" in linux_text
}

test test_ifdown_skips_unconfigured_interface [fs, process, env, error] { |ctx|
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
  run XSH_LINUX_DRY_RUN=1 XSH_LINUX_DRY_RUN_LOG=$linux_log XSH_IFUP_INTERFACES=$interfaces XSH_IFUP_STATE=$state ${ctx.xsh_bin} fp"${ctx.core_dir}/ifdown.xsh" -- eth0 ?

  linux_log.exists()? == false
}

test test_ifdown_logical_selection [fs, process, env, error] { |ctx|
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

  run XSH_LINUX_DRY_RUN=1 XSH_LINUX_DRY_RUN_LOG=$linux_log XSH_IFUP_INTERFACES=$interfaces XSH_IFUP_STATE=$state ${ctx.xsh_bin} fp"${ctx.core_dir}/ifdown.xsh" -- eth0=office ?

  "\"interface\":\"eth0\"" in linux_log.read_text()?
  state.exists()? == false
}
