#!/bin/xsh
use lib.dhcp_client as client
use lib.dhcp_live as live
use lib.dhcp_wire as wire

error IfupError = Usage : Usage | Config | Hook | State

type Interface = {
  logical: Str,
  family: Str,
  method: Str,
  address: Str,
  netmask: Str,
  gateway: Str,
  pre_up: List[Str],
  up: List[Str],
  post_up: List[Str],
}

type Config = {auto: List[Str], interfaces: List[Interface]}

type InterfaceSelection = {physical: Str, logical: Str}

pure empty_interface() -> Interface {
  let pre_up = []
  let up = []
  let post_up = []

  {
    logical: "",
    family: "",
    method: "",
    address: "",
    netmask: "",
    gateway: "",
    pre_up,
    up,
    post_up,
  }
}

pure empty_config() -> Config {
  let auto = []
  let interfaces: List[Interface] = []
  {auto, interfaces}
}

proc default_interfaces_path() [env] -> Result[Path] {
  let raw = env("XSH_IFUP_INTERFACES") ?? { |_|
    "/etc/network/interfaces"
  }
  fp"{raw}"
}

proc default_state_path() [env] -> Result[Path] {
  let raw = env("XSH_IFUP_STATE") ?? { |_|
    "/run/network/ifstate"
  }
  fp"{raw}"
}

pure first_word(line: Str) -> Str {
  let words = line.words()

  return "" when words.is_empty()

  words[0]
}

pure rest_after_word(line: Str) -> Str {
  let word = first_word(line)

  return "" when word == ""

  line.split("") |> drop(word.count_chars()).join("").trim()
}

pure add_unique(items: List[Str], item: Str) -> List[Str] {
  return items when item in items

  items.push(item)
}

pure glob_match(pattern: Str, text: Str) -> Bool {
  return true when pattern == "*"

  return pattern == text unless "*" in pattern

  let parts = pattern.split("*")

  return parts[1] in text when pattern.starts_with("*") and pattern.ends_with("*")

  return text.ends_with(parts[1]) when pattern.starts_with("*")

  return text.starts_with(parts[0]) when pattern.ends_with("*")

  text.starts_with(parts[0]) and text.ends_with(parts[1])
}

pure append_current(config: Config, current: Interface) -> Config {
  return config when current.logical == ""

  {...config, interfaces: config.interfaces.push(current)}
}

proc parse_source_path(source: Str, config: Config) [fs, error] -> Result[Config] {
  let path_value = fp"{source}"

  return parse_interfaces_file(path_value, config)? unless "*" in source

  let dir = path_value.parent()
  let pattern = path_value.name()
  var result = config

  return result unless dir.exists()

  for entry in fs.children(dir)?
    |> where .kind == "file" and glob_match(pattern, .name)
    |> sort-by .name {
    result = parse_interfaces_file(entry.path, result)?
  }

  result
}

proc parse_interfaces_file(path_value: Path, config: Config) [fs, error] -> Result[Config] {
  guard path_value.exists() else {
    return config
  }

  var result = config
  var current = empty_interface()

  for raw in path_value.lines()? {
    let line = raw.trim()
    continue when line == "" or line.starts_with("#")
    let fields = line.words()
    continue when fields.is_empty()

    match fields[0] {
      "source" => {
        result = append_current(result, current)
        current = empty_interface()

        if fields.len() != 2 {
          return Err(IfupError.Config(f"{path_value}: source expects one path"))
        }

        result = parse_source_path(fields[1], result)?
      }
      "source-directory" => {
        result = append_current(result, current)
        current = empty_interface()

        if fields.len() != 2 {
          return Err(IfupError.Config(f"{path_value}: source-directory expects one path"))
        }

        let dir = fp"{fields[1]}"

        if dir.exists() {
          for entry in fs.children(dir)?
            |> where .kind == "file"
            |> sort-by .name {
            result = parse_interfaces_file(entry.path, result)?
          }
        }
      }
      "auto" => {
        for name in fields |> drop(1) {
          result = {...result, auto: add_unique(result.auto, name)}
        }
      }
      "iface" => {
        result = append_current(result, current)

        if fields.len() < 4 {
          return Err(IfupError.Config(f"{path_value}: iface expects name, address family, and method"))
        }

        current = {...empty_interface(), logical: fields[1], family: fields[2], method: fields[3]}
      }
      "pre-up" => {
        if current.logical == "" {
          return Err(IfupError.Config(f"{path_value}: pre-up outside iface stanza"))
        }

        current = {...current, pre_up: current.pre_up.push(rest_after_word(line))}
      }
      "up" => {
        if current.logical == "" {
          return Err(IfupError.Config(f"{path_value}: up outside iface stanza"))
        }

        current = {...current, up: current.up.push(rest_after_word(line))}
      }
      "post-up" => {
        if current.logical == "" {
          return Err(IfupError.Config(f"{path_value}: post-up outside iface stanza"))
        }

        current = {...current, post_up: current.post_up.push(rest_after_word(line))}
      }
      "address" => {
        if fields.len() >= 2 and current.logical != "" {
          current = {...current, address: fields[1]}
        }
      }
      "netmask" => {
        if fields.len() >= 2 and current.logical != "" {
          current = {...current, netmask: fields[1]}
        }
      }
      "gateway" => {
        if fields.len() >= 2 and current.logical != "" {
          current = {...current, gateway: fields[1]}
        }
      }
      "mapping" | "allow-auto" | "allow-hotplug" => return Err(
        IfupError.Config(f"{path_value}: unsupported ifupdown directive {fields[0]}"),
      )
      else => {}
    }
  }

  append_current(result, current)
}

pure state_has_iface(state: Str, physical: Str) -> Bool {
  for line in state.lines() {
    let fields = line.words()

    return true when fields.len() >= 1 and fields[0].split("=")[0] == physical
  }

  false
}

proc mark_configured(state_path: Path, physical: Str, logical: Str) [fs, error] {
  let parent = state_path.parent()

  if ! parent.exists() {
    parent.mkdir()
  }

  var text = ""

  if state_path.exists() {
    text = state_path.read_text()?
  }

  return when state_has_iface(text, physical)

  if text != "" and ! text.ends_with("\n") {
    text = f"""{text}
"""
  }

  state_path.write_atomic(f"""{text}{physical}={logical}
""")
}

proc report_action(verbose: Bool, action: Str) [io, error] {
  if verbose { io.write_stdout(action + "\n") }
}

proc run_hook(command: Str, physical: Str, stanza: Interface, phase: Str, verbose: Bool) [process, error, io] {
  return when command == ""

  let env_record = {
    IFACE: physical,
    LOGICAL: stanza.logical,
    ADDRFAM: stanza.family,
    METHOD: stanza.method,
    MODE: "start",
    PHASE: phase,
    VERBOSITY: if verbose { "1" } else { "0" },
    IF_ADDRESS: stanza.address,
    IF_NETMASK: stanza.netmask,
    IF_GATEWAY: stanza.gateway,
  }

  report_action(verbose, f"hook {phase} {physical}: {command}")
  let status = process.run(process.command_argv("/bin/sh", ["sh", "-c", command], env: env_record))?

  if ! status.ok {
    return Err(IfupError.Hook(f"{phase} command failed for {physical}: {command}"))
  }
}

proc run_parts(dir: Path, physical: Str, stanza: Interface, phase: Str, verbose: Bool) [fs, process, error, io] {
  guard dir.exists() else {
    return
  }

  for entry in fs.children(dir)?
    |> where .kind == "file"
    |> sort-by .name {
    let env_record = {
      IFACE: physical,
      LOGICAL: stanza.logical,
      ADDRFAM: stanza.family,
      METHOD: stanza.method,
      MODE: "start",
      PHASE: phase,
      VERBOSITY: if verbose { "1" } else { "0" },
      IF_ADDRESS: stanza.address,
      IF_NETMASK: stanza.netmask,
      IF_GATEWAY: stanza.gateway,
    }

    report_action(verbose, f"hook {phase} {physical}: {entry.path}")
    let status = process.run(process.command_argv(entry.path, [entry.path], env: env_record))?

    return Err(IfupError.Hook(f"{entry.path} failed for {physical}")) unless status.ok
  }
}

proc find_stanza(config: Config, logical: Str) [error] -> Result[Interface] {
  for stanza in config.interfaces {
    return stanza when stanza.logical == logical
  }

  Err(IfupError.Config(f"unknown interface {logical}"))
}

proc configure_dhcp(physical: Str, wait_seconds: Int, verbose: Bool) [fs, process, net, env, time, error, io] {
  let settings = live.settings_from_environment(physical)?
  guard let held = live.lock_interface(settings)? else {
    report_action(verbose, f"dhcp {physical}: a supervised client owns the interface")
    live.set_admin_down(settings, false)
    ctx f"{physical}: the client holds the interface lock but its control socket does not answer" {
      live.ask(settings, "up")
    }
    let state = live.await_state(settings, ["init", "selecting", "requesting", "probing", "bound", "renewing", "rebinding", "init_reboot", "declined", "no_link"], wait_seconds * 1000)?
    guard let reached = state else {
      return Err(IfupError.State(f"{physical}: the supervised client did not answer within {wait_seconds}s"))
    }
    report_action(verbose, f"dhcp {physical}: client state {reached}")
    return
  }
  fs.unlock(held)
  report_action(verbose, f"link_up {physical}")
  linux.link_up(physical)
  report_action(verbose, f"dhcp_once {physical}")
  let session = live.supervise(settings, .OneShot(wait_seconds * 1000), null)?
  guard let lease = session.machine.lease else {
    return Err(IfupError.State(f"{physical}: no DHCP lease"))
  }
  report_action(verbose, f"dhcp {physical}: {wire.ip_text(lease.address)}")
}

proc configure_static(physical: Str, stanza: Interface, verbose: Bool) [process, error, io] {
  if stanza.address == "" or stanza.netmask == "" {
    return Err(IfupError.Config(f"{stanza.logical}: static inet stanza requires address and netmask"))
  }

  report_action(verbose, f"link_up {physical}")
  linux.link_up(physical)
  report_action(verbose, f"set_ipv4_address {physical} {stanza.address} {stanza.netmask}")
  linux.set_ipv4_address(physical, stanza.address, stanza.netmask)

  if stanza.gateway != "" {
    report_action(verbose, f"add_default_ipv4_route {physical} {stanza.gateway}")
    linux.add_default_ipv4_route(stanza.gateway, interface: physical)
  }
}

proc configure_interface(config: Config, state_path: Path, physical: Str, logical: Str, wait_seconds: Int, verbose: Bool) [fs, process, time, error, io] {
  return when state_path.exists() and state_has_iface(state_path.read_text()?, physical)

  let stanza = find_stanza(config, logical)?

  if stanza.family != "inet" {
    return Err(IfupError.Config(f"{stanza.logical}: unsupported address family {stanza.family}"))
  }

  for command in stanza.pre_up {
    run_hook(command, physical, stanza, "pre-up", verbose)
  }

  run_parts(/etc/network/if-pre-up.d, physical, stanza, "pre-up", verbose)

  match stanza.method {
    "loopback" | "manual" => {
      report_action(verbose, f"link_up {physical}")
      linux.link_up(physical)
    }
    "static" => configure_static(physical, stanza, verbose)
    "dhcp" => configure_dhcp(physical, wait_seconds, verbose)
    else => return Err(IfupError.Config(f"{stanza.logical}: unsupported method {stanza.method}"))
  }

  for command in stanza.up {
    run_hook(command, physical, stanza, "post-up", verbose)
  }

  for command in stanza.post_up {
    run_hook(command, physical, stanza, "post-up", verbose)
  }

  run_parts(/etc/network/if-up.d, physical, stanza, "post-up", verbose)
  mark_configured(state_path, physical, stanza.logical)
}

pure split_iface_arg(arg: Str) -> InterfaceSelection {
  let parts = arg.split("=", maxsplit: 1)

  return {physical: parts[0], logical: parts[1]} when parts.len() >= 2

  {physical: arg, logical: arg}
}

type IfupOptions = {all: Bool, verbose: Bool, patience: Str?, operands: List[Str]}

proc main(...argv: List[Str]) [fs, process, net, env, time, error, io] {
  let opts: IfupOptions = cli.applet(
    argv,
    {
      all: {
        form: "-a --all",
        default: false,
      },
      verbose: {
        form: "-v --verbose",
        default: false,
      },
      patience: {
        form: "--wait SECONDS",
      },
      operands: {
        form: "...INTERFACE",
      },
    },
  )?
  let {all, verbose, patience, operands} = opts
  let wait_seconds = if let text = patience { text as Int } else { 10 }

  if ! all and operands.is_empty() {
    return Err(IfupError.Usage("ifup: expected -a or interface name"))
  }

  let config = parse_interfaces_file(default_interfaces_path()?, empty_config())?
  let state_path = default_state_path()?

  if all {
    for name in config.auto {
      configure_interface(config, state_path, name, name, wait_seconds, verbose)
    }
  }

  for operand in operands {
    let selection = split_iface_arg(operand)
    configure_interface(config, state_path, selection.physical, selection.logical, wait_seconds, verbose)
  }
}
