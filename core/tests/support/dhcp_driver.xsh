##! Test support: runs one scenario inside the user, mount and network
##! namespaces its caller created. It builds the two-namespace world, then
##! carries out the scenario's steps in order: start and stop servers, recorders
##! and clients, take link actions, wait for files and write snapshots of the
##! client namespace. Every artifact lands in the scenario directory for the
##! test to read afterwards. Argument: the scenario file.
use lib.nettools_fixture as fixture
use tests.support.dhcp_types as types
use tests.support.dhcp_world as world



type Jobs = Map[Str, world.Job]

proc required(value: Str?, what: Str, action: Str) [error] -> Result[Str, Error] {
  if let present = value {
    Ok(present)
  } else {
    Err(error.failure(f"step {action} needs {what}"))
  }
}

proc wait_for(target: Path, needle: Str, timeout_ms: Int) [fs, time, error] {
  var waited = 0
  while waited < timeout_ms {
    if target.exists() {
      return when needle == ""
      return when needle in target.read_text()?
    }
    time.sleep(50ms)
    waited += 50
  }
  Err(error.failure(f"timed out waiting for {needle} in {target}"))
}

proc snapshot(target: Path) [fs, process, net, error] {
  let dump = linux.network_dump()?
  target.write(json.encode({state: dump.state, links: [{name: link.name, ifindex: link.ifindex, flags: link.flags} for link in dump.links], addresses: [{ifindex: item.ifindex, address: item.local ?? item.address ?? "", prefix: item.prefix_length, label: item.label} for item in dump.addresses if item.family == "inet"], routes: [{destination: route.destination, prefix: route.destination_prefix_length, gateway: route.gateway, ifindex: route.output_ifindex, table: route.table, tos: route.tos, priority: route.priority, protocol: route.protocol, scope: route.scope} for route in dump.routes if route.family == "inet"]})?)
}

proc perform(setup: world.World, step: types.Step, jobs: Jobs) [fs, process, net, time, error] -> Result[Jobs, Error] {
  let dir = setup.dir
  var table = jobs
  match step.do {
    "server_xsh" => {
      let script = required(step.file, "file", step.do)?
      let name = step.name ?? "server"
      table = table.set(name, world.start_support(setup, true, "dhcp_server", [world.SERVER_INTERFACE, script, fp"{dir}/{name}.requests".display()], fp"{dir}/{name}.log")?)
    }
    "server_stop" => {
      let name = step.name ?? "server"
      if let Ok(job) = table.get(name) {
        world.signal(job, step.signal ?? "KILL")
      }
    }
    "dnsmasq" => {
      let name = step.name ?? "dnsmasq"
      let flags = [
        "--keep-in-foreground",
        "--conf-file=/dev/null",
        "--no-resolv",
        "--no-hosts",
        "--port=0",
        "--user=root",
        "--group=root",
        f"--interface={world.SERVER_INTERFACE}",
        "--except-interface=lo",
        "--bind-interfaces",
        f"--dhcp-leasefile={dir}/dnsmasq.leases",
        f"--pid-file={dir}/dnsmasq.pid",
        "--log-dhcp",
        f"--log-facility={dir}/dnsmasq.log",
      ].extend(step.args)
      table = table.set(name, world.start_program(setup, true, "dnsmasq", flags, fp"{dir}/{name}.out")?)
    }
    "tcpdump" => {
      let side = step.side ?? "client"
      let file = required(step.file, "file", step.do)?
      let interface = if side == "server" { world.SERVER_INTERFACE } else { world.CLIENT_INTERFACE }
      let name = step.name ?? f"tcpdump-{side}"
      table = table.set(name, world.start_program(setup, side == "server", "tcpdump", ["-n", "-U", "--immediate-mode", "-s", "0", "-i", interface, "-w", fp"{dir}/{file}".display()], fp"{dir}/{name}.out")?)
    }
    "capture" => {
      let side = step.side ?? "client"
      let file = required(step.file, "file", step.do)?
      let interface = if side == "server" { world.SERVER_INTERFACE } else { world.CLIENT_INTERFACE }
      let name = step.name ?? f"capture-{side}"
      table = table.set(name, world.start_support(setup, side == "server", "dhcp_record", [interface, fp"{dir}/{file}".display()], fp"{dir}/{name}.log")?)
    }
    "client" => {
      let name = step.name ?? "client"
      table = table.set(name, world.start(setup, false, fp"{setup.core}/dhcpc.xsh", step.args, fp"{dir}/{name}.log")?)
    }
    "probe" => {
      let status = world.run_support(setup, false, "dhcp_probe", [world.CLIENT_INTERFACE, world.CLIENT_MAC, fp"{dir}/probe.json".display(), step.text ?? "12345"], fp"{dir}/probe.log")?
      fp"{dir}/probe.status".write(f"{status.shell_code()?}\n")
    }
    "sleep" => time.sleep((step.ms ?? 0) * 1ms)
    "wait_for" => wait_for(fp"{dir}/{required(step.file, "file", step.do)?}", step.text ?? "", step.ms ?? 5000)
    "kill" => {
      if let Ok(job) = table.get(step.name ?? "client") {
        world.signal(job, step.signal ?? "KILL")
      }
    }
    "link" => {
      if step.text == "down" {
        fixture.link_down(world.CLIENT_INTERFACE)
      } else {
        fixture.link_up(world.CLIENT_INTERFACE)
      }
    }
    "snapshot" => snapshot(fp"{dir}/{step.name ?? "snapshot"}.json")
    else => return Err(error.failure(f"unknown step {step.do}"))
  }
  Ok(table)
}

proc main(...argv: List[Str]) [fs, process, net, time, error] {
  let scenario = json.read(fp"{argv[0]}")?.require(types.Scenario)?
  let dir = fp"{scenario.dir}"
  let setup = world.build(dir, fp"{scenario.xsh}", fp"{scenario.core}")?
  var jobs: Jobs = map.empty()
  defer {
    for entry in jobs {
      world.signal(entry.value, "KILL")
    }
  }
  for step in scenario.steps {
    jobs = perform(setup, step, jobs)?
  }
}
