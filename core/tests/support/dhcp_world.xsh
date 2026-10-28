##! Test support: two network namespaces joined by a veth pair. The caller's
##! namespace holds the client end `eth0`; a second namespace, kept alive by a
##! bound namespace file, holds the server end `srv0` with the server address.
##! Everything runs inside user, mount and network namespaces the test created,
##! so the host's links, addresses, routes and resolver are never touched.
use lib.dhcp_wire as wire
use lib.nettools_fixture as fixture

## The client end of the pair.
export const CLIENT_INTERFACE = "eth0"
## The server end of the pair.
export const SERVER_INTERFACE = "srv0"
## The hardware address of the client end.
export const CLIENT_MAC = "02:00:00:00:00:01"
## The hardware address of the server end.
export const SERVER_MAC = "02:00:00:00:00:02"
## The address of the server end.
export const SERVER_ADDRESS = "192.168.50.1"
## The prefix length of the server subnet.
export const SERVER_PREFIX = 24

## Where a world lives and what runs it: `core` is the directory holding the
## `lib` and `tests/support` trees, `server_ns` the bound namespace file of the
## server side.
export type World = {dir: Path, xsh: Path, core: Path, server_ns: Path}

## One background program: its process group and the file its output goes to.
export type Job = {group: Int, log: Path}

proc support(world: World, name: Str) -> Path {
  fp"{world.core}/tests/support/{name}.xsh"
}

proc child_env(world: World) -> Record {
  {XSH_MODULE_PATH: world.core, LC_ALL: "C", XSH_EXECUTION_PHRASE: ""}
}

pure paths(items: List[Str]) -> List[Path] {
  [fp"{item}" for item in items]
}

proc command(world: World, argv: List[Path], log: Path) [fs, process, error] -> Result[Command] {
  process.command_argv(world.xsh, [world.xsh].extend(argv), world.dir, child_env(world), b"", log, log, timeout: 120s)
}

proc joined(world: World, argv: List[Path], log: Path) [fs, process, error] -> Result[Command] {
  let launcher = [support(world, "dhcp_join"), world.server_ns, world.xsh, world.xsh]
  process.command_argv(world.xsh, [world.xsh, @launcher, @argv], world.dir, child_env(world), b"", log, log, timeout: 120s)
}

## Builds the pair and brings the server end up in its own namespace. The
## caller must already be inside the user, mount and network namespaces the
## test created.
export proc build(dir: Path, xsh: Path, core: Path) [fs, process, net, error] -> Result[World, Error] {
  let server_ns = fp"{dir}/server.net"
  server_ns.write("")
  let world = World(dir:, xsh:, core:, server_ns:)
  let bind_log = fp"{dir}/bind.log"
  let binder = process.command_argv(xsh, [xsh, support(world, "dhcp_bind_netns"), server_ns], dir, child_env(world), b"", bind_log, bind_log)
  let bound = linux.run_in_namespaces(binder, unshare: ["net"])?
  if ! bound.exited_with(0) {
    return Err(error.failure(f"binding the server namespace failed: {bind_log.read_text()?}"))
  }
  let handle = unix.open_fd(server_ns)?
  defer unix.close_fd(handle)
  fixture.link_add_veth_into(CLIENT_INTERFACE, SERVER_INTERFACE, CLIENT_MAC, SERVER_MAC, handle)
  let link_log = fp"{dir}/server-link.log"
  let setup = process.command_argv(
    xsh,
    [xsh, support(world, "dhcp_join"), server_ns, xsh, xsh, support(world, "dhcp_server_link"), SERVER_INTERFACE, SERVER_ADDRESS, f"{SERVER_PREFIX}"],
    dir,
    child_env(world),
    b"",
    link_log,
    link_log,
  )
  let ready = process.run(setup)?
  if ! ready.exited_with(0) {
    return Err(error.failure(f"configuring the server end failed: {link_log.read_text()?}"))
  }
  world
}

## Starts `program` (a script path) with `args` in the background in its own
## process group, in the server namespace or the caller's, logging to `log`.
export proc start(world: World, in_server: Bool, program: Path, args: List[Str], log: Path) [fs, process, error] -> Result[Job, Error] {
  let argv = [program].extend(paths(args))
  let plan = if in_server { joined(world, argv, log)? } else { command(world, argv, log)? }
  let started = unix.spawn_process_group_log(plan, log)?
  Job(group: started.pid, log:)
}

## Starts an installed program in the background in its own process group, in
## the server namespace or the caller's, logging to `log`.
export proc start_program(world: World, in_server: Bool, program: Str, args: List[Str], log: Path) [fs, process, error] -> Result[Job, Error] {
  let plan = if in_server {
    process.command_argv(
      world.xsh,
      [world.xsh, support(world, "dhcp_join"), world.server_ns, fp"{program}", fp"{program}"].extend(paths(args)),
      world.dir,
      child_env(world),
      b"",
      log,
      log,
      timeout: 120s,
    )
  } else {
    process.command_argv(program, [fp"{program}"].extend(paths(args)), world.dir, child_env(world), b"", log, log, timeout: 120s)
  }
  let started = unix.spawn_process_group_log(plan, log)?
  Job(group: started.pid, log:)
}

## Starts a support program of this library in the background.
export proc start_support(world: World, in_server: Bool, name: Str, args: List[Str], log: Path) [fs, process, error] -> Result[Job, Error] {
  start(world, in_server, support(world, name), args, log)
}

## Signals every process of a job. A job already gone is not an error.
export proc signal(job: Job, name: Str) [process] {
  let _ = process.kill_group(job.group, name)
}

## Runs a support program in the foreground, in the server namespace or the
## caller's, and returns its status.
export proc run_support(world: World, in_server: Bool, name: Str, args: List[Str], log: Path) [fs, process, error] -> Result[Status, Error] {
  let argv = [support(world, name)].extend(paths(args))
  let plan = if in_server { joined(world, argv, log)? } else { command(world, argv, log)? }
  Ok(process.run(plan)?)
}

## Runs an XSH program to completion in the caller's namespace and returns its
## status, writing its output to `log`. The environment names the scenario
## directory's `interfaces` and `ifstate` files and the `lease`, `run` and
## `etc` directories the client was started with, so `ifup` and `ifdown`
## address the same state.
export proc run_program(world: World, program: Path, args: List[Str], log: Path) [fs, process, error] -> Result[Status, Error] {
  let dir = world.dir
  let environment = {
    XSH_MODULE_PATH: world.core,
    LC_ALL: "C",
    XSH_EXECUTION_PHRASE: "",
    XSH_IFUP_INTERFACES: fp"{dir}/interfaces",
    XSH_IFUP_STATE: fp"{dir}/ifstate",
    XSH_DHCPC_LEASE_DIR: fp"{dir}/lease",
    XSH_DHCPC_RUN_DIR: fp"{dir}/run",
    XSH_DHCPC_RESOLV_DIR: fp"{dir}/etc",
  }
  let plan = process.command_argv(world.xsh, [world.xsh, program, @paths(args)], dir, environment, b"", log, log, timeout: 120s)
  Ok(process.run(plan)?)
}
