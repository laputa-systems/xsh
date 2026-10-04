#!/bin/xsh
error IfdownError = Usage(message: Str) : Usage | Config(message: Str) | Hook(message: Str) | State(message: Str)

type Interface = {
  logical: Str,
  family: Str,
  method: Str,
  address: Str,
  netmask: Str,
  gateway: Str,
  pre_down: List[Str],
  down: List[Str],
  post_down: List[Str],
}

type Config = {auto: List[Str], interfaces: List[Interface]}

type InterfaceSelection = {physical: Str, logical: Str}

pure empty_interface() -> Interface {
  let pre_down = []
  let down = []
  let post_down = []

  {
    logical: "",
    family: "",
    method: "",
    address: "",
    netmask: "",
    gateway: "",
    pre_down,
    down,
    post_down,
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
  fp"${raw}"
}

proc default_state_path() [env] -> Result[Path] {
  let raw = env("XSH_IFUP_STATE") ?? { |_|
    "/run/network/ifstate"
  }
  fp"${raw}"
}

pure first_word(line: Str) -> Str {
  let words = line.words()

  return "" when words.len() == 0

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
  let path_value = fp"${source}"

  return parse_interfaces_file(path_value, config)? unless "*" in source

  let dir = path_value.parent()
  let pattern = path_value.name()
  var result = config

  return result unless dir.exists()?

  for entry in fs.children(dir)?
    |> where .kind == "file" and glob_match(pattern, .name)
    |> sort-by .name {
    result = parse_interfaces_file(entry.path, result)?
  }

  result
}

proc parse_interfaces_file(path_value: Path, config: Config) [fs, error] -> Result[Config] {
  guard path_value.exists()? else {
    return config
  }

  var result = config
  var current = empty_interface()

  for raw in path_value.lines()? {
    let line = raw.trim()
    continue when line == "" or line.starts_with("#")
    let fields = line.words()
    continue when fields.len() == 0

    match fields[0] {
      "source" => {
        result = append_current(result, current)
        current = empty_interface()

        if fields.len() != 2 {
          return Err(IfdownError.Config(f"${path_value}: source expects one path"))
        }

        result = parse_source_path(fields[1], result)?
      }
      "source-directory" => {
        result = append_current(result, current)
        current = empty_interface()

        if fields.len() != 2 {
          return Err(IfdownError.Config(f"${path_value}: source-directory expects one path"))
        }

        let dir = fp"${fields[1]}"

        if dir.exists()? {
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
          return Err(IfdownError.Config(f"${path_value}: iface expects name, address family, and method"))
        }

        current = {...empty_interface(), logical: fields[1], family: fields[2], method: fields[3]}
      }
      "pre-down" => {
        if current.logical == "" {
          return Err(IfdownError.Config(f"${path_value}: pre-down outside iface stanza"))
        }

        current = {...current, pre_down: current.pre_down.push(rest_after_word(line))}
      }
      "down" => {
        if current.logical == "" {
          return Err(IfdownError.Config(f"${path_value}: down outside iface stanza"))
        }

        current = {...current, down: current.down.push(rest_after_word(line))}
      }
      "post-down" => {
        if current.logical == "" {
          return Err(IfdownError.Config(f"${path_value}: post-down outside iface stanza"))
        }

        current = {...current, post_down: current.post_down.push(rest_after_word(line))}
      }
      "pre-up" | "up" | "post-up" => {}
      "address" => {
        # handled by ifup
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
      _ => {}
    }
  }

  append_current(result, current)
}

proc find_stanza(config: Config, logical: Str) [error] -> Result[Interface] {
  for stanza in config.interfaces {
    return stanza when stanza.logical == logical
  }

  Err(IfdownError.Config(f"unknown interface ${logical}"))
}

proc run_hook(command: Str, physical: Str, stanza: Interface, phase: Str) [process, error] {
  return when command == ""

  let env_record = {
    IFACE: physical,
    LOGICAL: stanza.logical,
    ADDRFAM: stanza.family,
    METHOD: stanza.method,
    MODE: "stop",
    PHASE: phase,
    VERBOSITY: "0",
    IF_ADDRESS: stanza.address,
    IF_NETMASK: stanza.netmask,
    IF_GATEWAY: stanza.gateway,
  }

  let status = process.run(process.command_argv("/bin/sh", ["sh", "-c", command], env: env_record))?

  if ! status.ok {
    return Err(IfdownError.Hook(f"${phase} command failed for ${physical}: ${command}"))
  }
}

proc run_parts(dir: Path, physical: Str, stanza: Interface, phase: Str) [fs, process, error] {
  guard dir.exists()? else {
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
      MODE: "stop",
      PHASE: phase,
      VERBOSITY: "0",
      IF_ADDRESS: stanza.address,
      IF_NETMASK: stanza.netmask,
      IF_GATEWAY: stanza.gateway,
    }

    let status = process.run(process.command_argv(entry.path, [entry.path.display()], env: env_record))?

    return Err(IfdownError.Hook(f"${entry.path} failed for ${physical}")) unless status.ok
  }
}

proc state_remove_iface(state_path: Path, physical: Str) [fs, error] {
  guard state_path.exists()? else {
    return
  }

  let text = state_path.read_text()?
  var new_lines = []

  for line in text.lines() {
    let fields = line.words()
    continue when fields.len() >= 1 and fields[0].split("=")[0] == physical
    new_lines += [line]
  }

  if new_lines.len() == 0 {
    state_path.remove()?
    return
  }

  state_path.write_atomic(new_lines.join("\n"))?
}

proc teardown_dhcp(physical: Str) [fs, process, error] {
  let interfaces = linux.interfaces()?
  var address = ""

  for iface in interfaces {
    if iface.name == physical {
      for addr in iface.addresses {
        if addr.family == "inet" and address == "" {
          address = addr.addr
        }
      }
    }
  }

  let routes = linux.routes()?

  for route in routes {
    if route.dst == "default" and route.dev == physical and "." in route.gateway {
      linux.del_default_ipv4_route(route.gateway, interface: physical)?
    }
  }

  if address != "" {
    linux.flush_ipv4_addresses(physical)?
  }

  linux.link_down(physical)?
}

proc teardown_static(physical: Str, stanza: Interface) [fs, process, error] {
  let routes = linux.routes()?

  for route in routes {
    if route.dst == "default" and route.dev == physical and "." in route.gateway {
      linux.del_default_ipv4_route(route.gateway, interface: physical)?
    }
  }

  if stanza.address != "" {
    linux.flush_ipv4_addresses(physical)?
  }

  linux.link_down(physical)?
}

proc deconfigure_interface(config: Config, state_path: Path, physical: Str, logical: Str) [fs, process, error] {
  guard state_path.exists()? else {
    return
  }

  let state_text = state_path.read_text()?
  var found = false

  for line in state_text.lines() {
    let fields = line.words()

    if fields.len() >= 1 and fields[0] == f"${physical}=${logical}" {
      found = true
    }
  }

  return unless found

  let stanza = find_stanza(config, logical)?

  if stanza.family != "inet" {
    return Err(IfdownError.Config(f"${stanza.logical}: unsupported address family ${stanza.family}"))
  }

  for command in stanza.pre_down {
    run_hook(command, physical, stanza, "pre-down")?
  }

  run_parts(/etc/network/if-pre-down.d, physical, stanza, "pre-down")?

  match stanza.method {
    "loopback" | "manual" => linux.link_down(physical)?
    "static" => teardown_static(physical, stanza)?
    "dhcp" => teardown_dhcp(physical)?
    _ => return Err(IfdownError.Config(f"${stanza.logical}: unsupported method ${stanza.method}"))
  }

  for command in stanza.down {
    run_hook(command, physical, stanza, "down")?
  }

  for command in stanza.post_down {
    run_hook(command, physical, stanza, "post-down")?
  }

  run_parts(/etc/network/if-post-down.d, physical, stanza, "post-down")?
  state_remove_iface(state_path, physical)?
}

pure split_iface_arg(arg: Str) -> InterfaceSelection {
  let parts = arg.split("=", maxsplit: 1)

  return {physical: parts[0], logical: parts[1]} when parts.len() >= 2

  {physical: arg, logical: arg}
}

type IfdownOptions = {all: Bool, operands: List[Str]}

stream state_configured_ifaces(state_path: Path) [fs, error] -> Stream[InterfaceSelection] {
  guard state_path.exists()? else {
    return
  }

  for line in state_path.read_text()?.lines() {
    let fields = line.words()

    if fields.len() >= 1 {
      let parts = fields[0].split("=", maxsplit: 1)

      yield {physical: parts[0], logical: parts[1]} when parts.len() >= 2
    }
  }
}

proc main(...argv: List[Str]) [fs, process, env, error] {
  let opts: IfdownOptions = cli.applet(
    argv,
    {
      all: {
        form: "-a --all",
        default: false,
      },
      ignored: {
        form: "-v --verbose",
        default: false,
      },
      operands: {
        form: "...INTERFACE",
      },
    },
  )?
  let {all, operands, ..} = opts

  if ! all and operands.len() == 0 {
    return Err(IfdownError.Usage("ifdown: expected -a or interface name"))
  }

  let config = parse_interfaces_file(default_interfaces_path()?, empty_config())?
  let state_path = default_state_path()?

  if all {
    for item in state_configured_ifaces(state_path) {
      deconfigure_interface(config, state_path, item.physical, item.logical)?
    }
  }

  for operand in operands {
    let selection = split_iface_arg(operand)
    deconfigure_interface(config, state_path, selection.physical, selection.logical)?
  }
}
