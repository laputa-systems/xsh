##! Test support: runs a scenario in private user, mount and network
##! namespaces from an ordinary test; a test already running as root (the
##! privileged Linux lane) omits the user namespace, because a server that drops
##! group privileges, like dnsmasq, cannot do so inside one. The scenario's steps are the driver's;
##! this module only writes the scenario file, enters the namespaces and
##! returns the directory the driver filled. A kernel that refuses the
##! namespaces skips the test with the reason, never passes it.
use tests.support.dhcp_types as types

## An empty step of the given action, to be refined with field updates.
export pure step(action: Str) -> types.Step {
  types.Step(do: action, name: null, file: null, ms: null, args: [], text: null, signal: null, side: null)
}

## Starts the scripted XSH server with the script file `script`.
export pure server_xsh(script: Path) -> types.Step {
  {...step("server_xsh"), file: script.display()}
}

## Starts dnsmasq on the server end with the extra command-line `flags`.
export pure dnsmasq(flags: List[Str]) -> types.Step {
  {...step("dnsmasq"), args: flags}
}

## Records one side's frames into the pcap `file` with tcpdump.
export pure tcpdump(side: Str, file: Str) -> types.Step {
  {...step("tcpdump"), side: side, file: file}
}

## Records the frames of one side into the pcap `file`.
export pure capture(side: Str, file: Str) -> types.Step {
  {...step("capture"), side: side, file: file}
}

## Waits `ms` milliseconds.
export pure sleep(ms: Int) -> types.Step {
  {...step("sleep"), ms: ms}
}

## Waits until `file` contains `text`, for at most `ms` milliseconds.
export pure wait_for(file: Str, text: Str, ms: Int) -> types.Step {
  {...step("wait_for"), file: file, text: text, ms: ms}
}

## Runs the minimal probe client with the transaction id `xid`.
export pure probe(xid: Int) -> types.Step {
  {...step("probe"), text: f"{xid}"}
}

## Writes a snapshot of the client namespace named `name`.
export pure snapshot(name: Str) -> types.Step {
  {...step("snapshot"), name: name}
}

## Runs `steps` in a fresh world below the test's temporary directory and
## returns that directory, or null after skipping the test.
export proc execute(ctx: TestContext, name: Str, steps: List[types.Step]) [fs, process, error] -> Result[Path?, Error] {
  let dir = test.temp_dir(ctx, name:)?
  let scenario = types.Scenario(xsh: ctx.xsh_bin.display(), core: ctx.core_dir.display(), dir: dir.display(), steps:)
  let file = fp"{dir}/scenario.json"
  file.write(json.encode(scenario)?)
  let log = fp"{dir}/driver.log"
  let plan = process.command_argv(
    ctx.xsh_bin,
    [ctx.xsh_bin, fp"{ctx.core_dir}/tests/support/dhcp_driver.xsh", file],
    dir,
    {XSH_MODULE_PATH: ctx.core_dir, LC_ALL: "C", XSH_EXECUTION_PHRASE: ""},
    b"",
    log,
    log,
    timeout: 120s,
  )
  let privileged = unix.id()?.uid == 0
  let kinds = if privileged { ["mnt", "net"] } else { ["user", "mnt", "net"] }
  match linux.run_in_namespaces(plan, unshare: kinds, map_root_user: ! privileged) {
    Ok(status) => {
      return Ok(dir) when status.exited_with(0)
      let text = log.read_text()?
      if "cannot" in text or "Operation not permitted" in text {
        test.skip(f"cannot build the namespaces: {text.trim()}")
        return Ok(null)
      }
      Err(error.failure(f"scenario {name} failed ({status.shell_code()?}): {text}"))
    }
    Err(failure) => {
      test.skip(f"the kernel refuses user, mount and network namespaces: {failure.message}")
      Ok(null)
    }
  }
}
