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

pure expand(items: List[Str], base: Path) -> List[Str] {
  [item.replace("{dir}", with: base.display()) for item in items]
}

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

type Sample = {ticks: Int, rss_kb: Int, voluntary_switches: Int, involuntary_switches: Int, fds: Int, threads: Int}

pure status_value(text: Str, key: Str) -> Result[Int, Error] {
  for line in text.lines() {
    if line.starts_with(f"{key}:") {
      return Ok(line.split(":")[1].trim().split(" ")[0] as Int)
    }
  }
  Ok(0)
}

proc sample_process(pid: Int) [fs, error] -> Result[Sample, Error] {
  let stat = fp"/proc/{pid}/stat".read_text()?
  let fields = stat.split(") ")[1].split(" ")
  let status = fp"/proc/{pid}/status".read_text()?
  var voluntary = 0
  var involuntary = 0
  for task in fs.children(fp"/proc/{pid}/task")? {
    let task_status = fp"{task.path}/status".read_text()?
    voluntary += status_value(task_status, "voluntary_ctxt_switches")?
    involuntary += status_value(task_status, "nonvoluntary_ctxt_switches")?
  }
  Sample(
    ticks: fields[11] as Int + fields[12] as Int,
    rss_kb: status_value(status, "VmRSS")?,
    voluntary_switches: voluntary,
    involuntary_switches: involuntary,
    fds: [child for child in fs.children(fp"/proc/{pid}/fd")?].len(),
    threads: status_value(status, "Threads")?,
  )
}

proc measure(job: world.Job, window_ms: Int, target: Path) [fs, time, error] {
  let before = sample_process(job.group)?
  time.sleep(window_ms * 1ms)
  let after = sample_process(job.group)?
  target.write(json.encode({window_ms: window_ms, before: before, after: after})?)
}

type Clocks = {monotonic_ms: Int, boottime_ms: Int, wall_ms: Int, rtc_ms: Int}

proc read_clocks() [fs, process, time, error] -> Result[Clocks, Error] {
  Clocks(monotonic_ms: time.monotonic()? / 1000000, boottime_ms: time.boottime()? / 1000000, wall_ms: time.now(), rtc_ms: linux.hwclock()?)
}

proc sample_clocks(target: Path, window_ms: Int, period_ms: Int) [fs, process, time, error] {
  var samples: List[Clocks] = []
  let started = time.monotonic()? / 1000000
  while time.monotonic()? / 1000000 - started < window_ms {
    samples += [read_clocks()?]
    time.sleep(period_ms * 1ms)
  }
  target.write(json.encode(samples)?)
}

proc announce(text: Str) [fs, io, error] {
  print f"VMMARK {text}"
  io.flush_stdout()?
}

proc run_fixture(action: Str, args: List[Str]) [process, net, error] {
  match action {
    "link_add_dummy" => fixture.link_add_dummy(args[0], args[1])
    "link_up" => fixture.link_up(args[0])
    "link_down" => fixture.link_down(args[0])
    "address_add4" => fixture.address_add4(args[0], args[1], args[2] as Int, args[3], args[4])
    "route_add_type4" => fixture.route_add_type4(args[0], args[1] as Int, args[2] as Int)
    "route_add_tos4" => fixture.route_add_tos4(args[0], args[1] as Int, args[2] as Int, args[3])
    else => return Err(error.failure(f"unknown fixture action {action}"))
  }
}

proc perform(setup: world.World, step: types.Step, jobs: Jobs) [fs, io, process, net, time, error] -> Result[Jobs, Error] {
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
      table = table.set(name, world.start(setup, false, fp"{setup.core}/dhcpc.xsh", expand(step.args, dir), fp"{dir}/{name}.log")?)
    }
    "probe" => {
      let status = world.run_support(setup, false, "dhcp_probe", [world.CLIENT_INTERFACE, world.CLIENT_MAC, fp"{dir}/probe.json".display(), step.text ?? "12345"], fp"{dir}/probe.log")?
      fp"{dir}/probe.status".write(f"{status.shell_code()?}\n")
    }
    "applet" => {
      let script = required(step.file, "file", step.do)?
      let tag = step.name ?? script
      let status = world.run_program(setup, fp"{setup.core}/{script}.xsh", expand(step.args, dir), fp"{dir}/{tag}.out")?
      fp"{dir}/{tag}.status".write(f"{status.shell_code()?}\n")
    }
    "write" => {
      let target = fp"{dir}/{required(step.file, "file", step.do)?}"
      if ! target.parent().exists() {
        target.parent().mkdir()
      }
      target.write(step.text ?? "")
    }
    "copy" => fp"{dir}/{step.text ?? ""}".write(fp"{dir}/{required(step.file, "file", step.do)?}".read_bytes()?)
    "fixture" => run_fixture(required(step.file, "file", step.do)?, step.args)
    "recreate" => {
      fixture.link_delete(world.CLIENT_INTERFACE)
      let handle = unix.open_fd(setup.server_ns)?
      defer unix.close_fd(handle)
      fixture.link_add_veth_into(world.CLIENT_INTERFACE, world.SERVER_INTERFACE, world.CLIENT_MAC, world.SERVER_MAC, handle)
      let relink = world.run_support(setup, true, "dhcp_server_link", [world.SERVER_INTERFACE, world.SERVER_ADDRESS, f"{world.SERVER_PREFIX}"], fp"{dir}/recreate.log")?
      fp"{dir}/recreate.status".write(f"{relink.shell_code()?}\n")
    }
    "mark" => announce(step.text ?? "")?
    "sample" => sample_clocks(fp"{dir}/{required(step.file, "file", step.do)?}", step.ms ?? 10000, 100)?
    "wall_step" => linux.set_system_clock(time.now() + (step.ms ?? 0))?
    "measure" => {
      guard let job = table.get(step.name ?? "client") else { return Err(error.failure("no such job to measure")) }
      measure(job, step.ms ?? 10000, fp"{dir}/measure-{step.name ?? "client"}.json")?
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

proc main(...argv: List[Str]) [fs, io, process, net, time, error] {
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
