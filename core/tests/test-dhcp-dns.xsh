use core.lib.dhcp_dns as dns_owner
use core.lib.dhcp_machine as machine
use core.lib.dhcp_wire as wire

const MARKER = "# Managed by dhcpc: edit the static file, not this one."

type Setup = {layout: dns_owner.Layout, state: Path, etc: Path, resolv: Path, static: Path}

pure empty_static() -> dns_owner.Directives {
  {servers: [], search: [], options: []}
}

pure contribution(interface: Str, servers: List[Str], search: List[Str]) -> dns_owner.Contribution {
  {interface: interface, servers: servers, search: search}
}

proc setup(ctx: TestContext, name: Str) [fs, process, error] -> Result[Setup] {
  let root = test.temp_dir(ctx, name:)?
  let state = fp"{root}/state"
  let etc = fp"{root}/etc"
  state.mkdir()
  etc.mkdir()
  let static = fp"{etc}/resolv.static"
  let layout = dns_owner.Layout(run_dir: state, destination_dir: etc, destination: "resolv.conf", static_path: static, lock_wait_ms: 200)
  {layout: layout, state: state, etc: etc, resolv: fp"{etc}/resolv.conf", static: static}
}

test test_composition_puts_static_first_then_interfaces_in_name_order {
  let static = dns_owner.parse_static("# local resolver\nnameserver 10.0.0.53\nsearch corp.example\noptions timeout:2\n")
  let text = dns_owner.compose(
    static,
    [contribution("eth1", ["192.168.1.1"], ["lan.example"]), contribution("eth0", ["192.168.0.1", "10.0.0.53"], ["wan.example", "corp.example"])],
  )
  assert text == f"{MARKER}\noptions timeout:2\nnameserver 10.0.0.53\nnameserver 192.168.0.1\nnameserver 192.168.1.1\nsearch corp.example wan.example lan.example\n"
}

test test_composition_is_bounded_and_deduplicated {
  let many = dns_owner.compose(
    empty_static(),
    [contribution("a", ["1.1.1.1", "1.1.1.1", "2.2.2.2"], []), contribution("b", ["3.3.3.3", "4.4.4.4"], []), contribution("c", ["5.5.5.5"], [])],
  )
  assert many == f"{MARKER}\nnameserver 1.1.1.1\nnameserver 2.2.2.2\nnameserver 3.3.3.3\n"
  let full = dns_owner.parse_static("nameserver 8.8.8.8\nnameserver 8.8.4.4\nnameserver 9.9.9.9\n")
  assert "nameserver 1.1.1.1" not in dns_owner.compose(full, [contribution("eth0", ["1.1.1.1"], [])])
  assert dns_owner.compose(empty_static(), []) == f"{MARKER}\n"
  let names = [f"d{index}.example" for index in range(1, 10)]
  let wide = dns_owner.compose(empty_static(), [contribution("eth0", [], names[..6]), contribution("eth1", [], names[6..])])
  assert wide == f"{MARKER}\nsearch d1.example d2.example d3.example d4.example d5.example d6.example\n"
}

test test_static_parsing_keeps_only_safe_directives {
  let parsed = dns_owner.parse_static(
    """# comment
nameserver 10.0.0.53
nameserver 10.0.0.53; reboot
nameserver
nameserver 2001:db8::53
nameserver not-an-address
search a.example b.example bad..name -x.example
domain c.example
options ndots:2 timeout:1 evil$(x)
lookup file bind
sortlist 10.0.0.0
""",
  )
  assert parsed.servers == ["10.0.0.53", "2001:db8::53"]
  assert parsed.search == ["a.example", "b.example", "c.example"]
  assert parsed.options == ["ndots:2", "timeout:1"]
}

test test_hostile_values_never_reach_a_contribution {
  for servers in [["1.1.1.1\nnameserver 6.6.6.6"], ["1.1.1.1 "], ["999.1.1.1"], [""], ["1.1.1.1", "2.2.2.2", "3.3.3.3", "4.4.4.4"]] {
    assert ! dns_owner.contribution_ok(contribution("eth0", servers, []))
  }
  for search in [["a.example\noptions edns0"], ["a b"], ["a..b"], [""], ["x.example", "y.example", "z.example", "a.example", "b.example", "c.example", "d.example"], ["-bad.example"]] {
    assert ! dns_owner.contribution_ok(contribution("eth0", [], search))
  }
  for interface in ["", "..", "a/b", "eth0\n", "abcdefghijklmnop"] {
    assert ! dns_owner.contribution_ok(contribution(interface, [], []))
  }
  assert dns_owner.contribution_ok(contribution("eth0.100", ["1.1.1.1", "2001:db8::1"], ["lan.example"]))
}

test test_contribution_files_round_trip_and_refuse_everything_else {
  let item = contribution("eth0", ["1.1.1.1"], ["lan.example"])
  assert dns_owner.decode_contribution("eth0", dns_owner.encode_contribution(item)?)? == item
  let good = dns_owner.encode_contribution(item)?
  for text in ["", "x", "[]", good.replace("\"version\":1", with: "\"version\":2"), good.replace("\"version\":1", with: "\"version\":1,\"x\":1"), good.replace("1.1.1.1", with: "1.1.1.1\\n2.2.2.2")] {
    assert dns_owner.decode_contribution("eth0", text) is Err(_), text
  }
}

pure lease_with(servers: List[Int], domain: Str?) -> machine.Lease {
  machine.Lease(
    address: 3232235876,
    prefix: 24,
    broadcast: 3232236031,
    routers: [3232235777],
    classless: [],
    dns: servers,
    domain:,
    server: 3232235777,
    lease_secs: 600,
    acquired_at: 0,
    renew_at: null,
    rebind_at: null,
    expire_at: null,
    timers: machine.Infinite,
  )
}

test test_a_lease_contributes_its_servers_and_domain {
  let item = dns_owner.contribution_for("eth0", lease_with([167772161, 167772162, 167772163, 167772164], "lan.example"))
  assert item.servers == ["10.0.0.1", "10.0.0.2", "10.0.0.3"] and item.search == ["lan.example"]
  assert dns_owner.contribution_for("eth0", lease_with([], null)) == contribution("eth0", [], [])
}

test test_updates_publish_with_the_marker_and_removal_recomposes { |ctx|
  let fx = setup(ctx, name: "dns-publish")?
  fx.static.write("nameserver 10.0.0.53\noptions timeout:2\n")
  let standing = dns_owner.update(fx.layout, "eth0", contribution("eth0", ["192.168.0.1"], ["wan.example"]))?
  assert standing == .Published
  assert fx.resolv.read_text()? == f"{MARKER}\noptions timeout:2\nnameserver 10.0.0.53\nnameserver 192.168.0.1\nsearch wan.example\n"
  assert fx.resolv.metadata()?.mode.bit_and(0o7777) == 0o644
  assert dns_owner.update(fx.layout, "eth1", contribution("eth1", ["192.168.1.1"], []))? == .Published
  assert "nameserver 192.168.1.1" in fx.resolv.read_text()?
  assert dns_owner.update(fx.layout, "eth1", contribution("eth1", ["192.168.1.1"], []))? == .Unchanged

  assert dns_owner.update(fx.layout, "eth0", null)? == .Published
  let after = fx.resolv.read_text()?
  assert "192.168.0.1" not in after and "wan.example" not in after and "192.168.1.1" in after and "10.0.0.53" in after
  assert fp"{fx.state}/dns/eth1.dns".exists()? and ! fp"{fx.state}/dns/eth0.dns".exists()?
  assert dns_owner.update(fx.layout, "eth0", null)? == .Unchanged
  assert dns_owner.update(fx.layout, "eth1", null)? == .Published
  assert fx.resolv.read_text()? == f"{MARKER}\noptions timeout:2\nnameserver 10.0.0.53\n"
}

test test_a_foreign_regular_file_survives_byte_for_byte { |ctx|
  let fx = setup(ctx, name: "dns-foreign")?
  let original = "# resolv.conf(5) file generated by tailscale\n# DO NOT EDIT THIS FILE BY HAND -- your changes will be overwritten\nnameserver 100.100.100.100\nsearch tail1234.ts.net\n"
  fx.resolv.write(original)
  let standing = dns_owner.update(fx.layout, "eth0", contribution("eth0", ["192.168.0.1"], []))?
  assert standing == .Conflict("foreign_file")
  assert fx.resolv.read_text()? == original
  assert fp"{fx.state}/dns/eth0.dns".exists()?, "the contribution waits for the file to be released"
  assert dns_owner.update(fx.layout, "eth0", null)? == .Conflict("foreign_file")
  assert fx.resolv.read_text()? == original
  assert ! fp"{fx.state}/dns/eth0.dns".exists()?

  assert dns_owner.update(fx.layout, "eth1", contribution("eth1", ["192.168.1.1"], []))? == .Conflict("foreign_file")
  fx.resolv.remove()
  assert dns_owner.update(fx.layout, "eth2", contribution("eth2", ["192.168.2.1"], []))? == .Published
  let released = fx.resolv.read_text()?
  assert "192.168.1.1" in released and "192.168.2.1" in released, "contributions kept while the file was foreign are published once it is released"
}

test test_a_foreign_symlink_and_other_entries_are_never_replaced { |ctx|
  let fx = setup(ctx, name: "dns-symlink")?
  fp"{fx.etc}/stub-resolv.conf".write("nameserver 127.0.0.53\n")
  fx.resolv.symlink(to: p"stub-resolv.conf")
  assert dns_owner.update(fx.layout, "eth0", contribution("eth0", ["192.168.0.1"], []))? == .Conflict("symlink")
  assert fx.resolv.readlink()? == p"stub-resolv.conf"
  assert fp"{fx.etc}/stub-resolv.conf".read_text()? == "nameserver 127.0.0.53\n"
  fx.resolv.remove()
  fx.resolv.mkdir()
  assert dns_owner.update(fx.layout, "eth0", contribution("eth0", ["192.168.0.1"], []))? == .Conflict("special_entry")
  assert fx.resolv.is_dir()?
  fx.resolv.remove()
  fx.resolv.symlink(to: p"missing-target")
  assert dns_owner.update(fx.layout, "eth0", contribution("eth0", ["192.168.0.1"], []))? == .Conflict("symlink")
}

test test_a_file_we_published_is_replaced_even_after_the_contents_are_edited_below_the_marker { |ctx|
  let fx = setup(ctx, name: "dns-ours")?
  assert dns_owner.update(fx.layout, "eth0", contribution("eth0", ["192.168.0.1"], []))? == .Published
  fx.resolv.write(f"{MARKER}\nnameserver 6.6.6.6\n")
  assert dns_owner.update(fx.layout, "eth0", contribution("eth0", ["192.168.0.1"], []))? == .Published
  assert "6.6.6.6" not in fx.resolv.read_text()?
}

test test_updates_are_serialized_by_one_lock { |ctx|
  let fx = setup(ctx, name: "dns-lock")?
  fp"{fx.state}/dns".mkdir()
  let held = fs.lock(fp"{fx.state}/dns/dns.lock")?
  match dns_owner.update(fx.layout, "eth0", contribution("eth0", ["192.168.0.1"], [])) {
    Ok(_) => assert false, "an update must wait for the lock holder"
    Err(error) => assert "lock" in error.message
  }
  assert ! fx.resolv.exists()?
  assert ! fp"{fx.state}/dns/eth0.dns".exists()?, "a refused update changes nothing"
  fs.unlock(held)
  assert dns_owner.update(fx.layout, "eth0", contribution("eth0", ["192.168.0.1"], []))? == .Published
}

test test_concurrent_processes_converge_on_one_consistent_file { |ctx|
  let fx = setup(ctx, name: "dns-concurrent")?
  let source = f"""use lib.dhcp_dns as dns_owner

proc main(...argv: List[Str]) [fs, time, error] {{
  let layout = dns_owner.Layout(run_dir: fp"{{argv[0]}}", destination_dir: fp"{{argv[1]}}", destination: "resolv.conf", static_path: null, lock_wait_ms: 20000)
  let interface = argv[2]
  repeat 5 times {{
    let _ = dns_owner.update(layout, interface, dns_owner.Contribution(interface:, servers: [argv[3]], search: []))?
  }}
}}
"""
  let script = test.temp_file(ctx, name: "update.xsh", contents: bytes.from_text(source))?
  let names = ["eth0", "eth1", "eth2", "eth3", "eth4", "eth5"]
  let child_env = {XSH_MODULE_PATH: ctx.core_dir.display()}
  let statuses = names
    |> par-map { |name|
      let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin, script, fx.state, fx.etc, name, f"10.0.0.{name.byte_slice(3)}"], env: child_env)
      process.run(plan)
    }
  for status in statuses {
    assert status?.ok
  }
  let text = fx.resolv.read_text()?
  assert text == f"{MARKER}\nnameserver 10.0.0.0\nnameserver 10.0.0.1\nnameserver 10.0.0.2\n", text
  for name in names {
    assert fp"{fx.state}/dns/{name}.dns".exists()?
  }
  for entry in fs.children(fx.etc)? {
    assert ".tmp." not in entry.name
  }
}
