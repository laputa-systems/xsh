#!/bin/xsh
error IfupError = Usage(message: Str) : Usage | Config(message: Str) | Hook(message: Str) | State(message: Str)

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

# Minimal IPv4 DHCP client (RFC 2131), modeled on busybox udhcpc but pared down
# to the DISCOVER/OFFER/REQUEST/ACK handshake. The broadcast UDP socket is
# provided by the linux.dhcp_* primitives; everything else is plain byte work.
const DHCP_MAGIC = [99, 130, 83, 99]
const DHCP_DISCOVER = 1
const DHCP_OFFER = 2
const DHCP_REQUEST = 3
const DHCP_ACK = 5
const DHCP_HEADER_LEN = 240
const DHCP_RETRIES = 5
const DHCP_TIMEOUT_MS = 3000

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

  (line.split("") |> drop(word.count_chars())).join("").trim()
}

pure add_unique(items: List[Str], item: Str) -> List[Str] {
  return items when item in items

  items.push(item)
}

pure glob_match(pattern: Str, text: Str) -> Bool {
  return true when pattern == "*"

  return pattern == text unless ("*" in pattern)

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

  return parse_interfaces_file(path_value, config)? unless ("*" in source)

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
          return Err(IfupError.Config(f"${path_value.display()}: source expects one path"))
        }

        result = parse_source_path(fields[1], result)?
      }
      "source-directory" => {
        result = append_current(result, current)
        current = empty_interface()

        if fields.len() != 2 {
          return Err(IfupError.Config(f"${path_value.display()}: source-directory expects one path"))
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
          return Err(IfupError.Config(f"${path_value.display()}: iface expects name, address family, and method"))
        }

        current = {...empty_interface(), logical: fields[1], family: fields[2], method: fields[3]}
      }
      "pre-up" => {
        if current.logical == "" {
          return Err(IfupError.Config(f"${path_value.display()}: pre-up outside iface stanza"))
        }

        current = {...current, pre_up: current.pre_up.push(rest_after_word(line))}
      }
      "up" => {
        if current.logical == "" {
          return Err(IfupError.Config(f"${path_value.display()}: up outside iface stanza"))
        }

        current = {...current, up: current.up.push(rest_after_word(line))}
      }
      "post-up" => {
        if current.logical == "" {
          return Err(IfupError.Config(f"${path_value.display()}: post-up outside iface stanza"))
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
        IfupError.Config(f"${path_value.display()}: unsupported ifupdown directive ${fields[0]}"),
      )
      _ => {}
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

  if ! parent.exists()? {
    parent.mkdir()?
  }

  var text = ""

  if state_path.exists()? {
    text = state_path.read_text()?
  }

  return when state_has_iface(text, physical)

  if text != "" and ! text.ends_with("\n") {
    text = f"""${text}
"""
  }

  state_path.write_atomic(f"""${text}${physical}=${logical}
""")?
}

proc run_hook(command: Str, physical: Str, stanza: Interface, phase: Str) [process, error] {
  return when command == ""

  let env_record = {
    IFACE: physical,
    LOGICAL: stanza.logical,
    ADDRFAM: stanza.family,
    METHOD: stanza.method,
    MODE: "start",
    PHASE: phase,
    VERBOSITY: "0",
    IF_ADDRESS: stanza.address,
    IF_NETMASK: stanza.netmask,
    IF_GATEWAY: stanza.gateway,
  }

  let status = process.run(process.command_argv("/bin/sh", ["sh", "-c", command], env: env_record))?

  if ! status.ok {
    return Err(IfupError.Hook(f"${phase} command failed for ${physical}: ${command}"))
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
      MODE: "start",
      PHASE: phase,
      VERBOSITY: "0",
      IF_ADDRESS: stanza.address,
      IF_NETMASK: stanza.netmask,
      IF_GATEWAY: stanza.gateway,
    }

    let status = process.run(process.command_argv(entry.path, [entry.path.display()], env: env_record))?

    if ! status.ok {
      return Err(IfupError.Hook(f"${entry.path.display()} failed for ${physical}"))
    }
  }
}

proc find_stanza(config: Config, logical: Str) [error] -> Result[Interface] {
  for stanza in config.interfaces {
    return stanza when stanza.logical == logical
  }

  Err(IfupError.Config(f"unknown interface ${logical}"))
}

pure hex_nibble(code: Int) -> Int {
  return code - 48 when 48 <= code <= 57

  return code - 87 when 97 <= code <= 102

  return code - 55 when 65 <= code <= 70

  0
}

pure parse_mac(mac: Str) -> List[Int] {
  [hex_nibble((part.byte_at(0) ?? -1)) * 16 + hex_nibble((part.byte_at(1) ?? -1)) for part in mac.split(":") if part != ""]
}

type DhcpLease = {valid: Bool, message_type: Int, yiaddr: List[Int], netmask: Str, gateway: Str, dns: List[Str], server_id: List[Int]}

pure empty_lease() -> DhcpLease {
  let yiaddr = []
  let dns_servers = []
  let server_id = []

  {
    valid: false,
    message_type: 0,
    yiaddr,
    netmask: "",
    gateway: "",
    dns: dns_servers,
    server_id,
  }
}

pure ints_to_ip(octets: List[Int]) -> Str {
  guard octets.len() == 4 else {
    return ""
  }

  f"${octets[0]}.${octets[1]}.${octets[2]}.${octets[3]}"
}

proc read_ip_octets(packet: Bytes, offset: Int) [error] -> Result[List[Int]] {
  [
    bytes.unpack_be(packet, 1, offset)?,
    bytes.unpack_be(packet, 1, offset + 1)?,
    bytes.unpack_be(packet, 1, offset + 2)?,
    bytes.unpack_be(packet, 1, offset + 3)?,
  ]
}

# Build a BOOTREQUEST. requested_ip/server_id are 4-byte lists for REQUEST and
# empty for DISCOVER; the broadcast flag asks the server to broadcast its reply,
# which is required while the interface still has no address.
proc dhcp_packet(
  msg_type: Int,
  xid: Int,
  mac: List[Int],
  requested_ip: List[Int],
  server_id: List[Int],
) [error] -> Result[Bytes] {
  var chunks = []
  chunks = chunks.push(bytes.from_ints([1, 1, 6, 0])?)
  chunks = chunks.push(bytes.pack_be(xid, 4)?)
  chunks = chunks.push(bytes.from_ints([0, 0])?)
  chunks = chunks.push(bytes.from_ints([128, 0])?)
  chunks = chunks.push(bytes.zero(16)?)
  chunks = chunks.push(bytes.from_ints(mac)?)
  chunks = chunks.push(bytes.zero(16 - mac.len())?)
  chunks = chunks.push(bytes.zero(192)?)
  chunks = chunks.push(bytes.from_ints(DHCP_MAGIC)?)
  var options = [53, 1, msg_type, 61, 7, 1].extend(mac)

  if requested_ip.len() == 4 {
    options = [@options, 50, 4, @requested_ip]
  }

  if server_id.len() == 4 {
    options = [@options, 54, 4, @server_id]
  }

  options = options.extend([55, 5, 1, 3, 6, 15, 28]).push(255)
  chunks = chunks.push(bytes.from_ints(options)?)
  bytes.concat(chunks)
}

proc parse_dhcp_reply(packet: Bytes, xid: Int) [error] -> Result[DhcpLease] {
  let total = packet.len()

  return empty_lease() when total < DHCP_HEADER_LEN

  if bytes.unpack_be(packet, 1, 0)? != 2 or bytes.unpack_be(packet, 4, 4)? != xid {
    return empty_lease()
  }

  let yiaddr = read_ip_octets(packet, 16)?
  var message_type = 0
  var netmask = ""
  var gateway = ""
  var dns_servers = []
  var server_id = []
  var pos = DHCP_HEADER_LEN

  while pos < total {
    let tag = bytes.unpack_be(packet, 1, pos)?

    if tag == 0 {
      pos = pos + 1
      continue
    }

    break when tag == 255
    let len = bytes.unpack_be(packet, 1, pos + 1)?
    let value = pos + 2

    if tag == 53 {
      message_type = bytes.unpack_be(packet, 1, value)?
    } else if tag == 1 {
      netmask = ints_to_ip(read_ip_octets(packet, value)?)
    } else if tag == 3 {
      gateway = ints_to_ip(read_ip_octets(packet, value)?)
    } else if tag == 54 {
      server_id = read_ip_octets(packet, value)?
    } else if tag == 6 {
      var offset = 0

      while offset + 4 <= len {
        dns_servers = dns_servers.push(ints_to_ip(read_ip_octets(packet, value + offset)?))
        offset = offset + 4
      }
    }

    pos = value + len
  }

  {
    valid: true,
    message_type,
    yiaddr,
    netmask,
    gateway,
    dns: dns_servers,
    server_id,
  }
}

# Drive the handshake on `physical` and return the acknowledged lease.
proc dhcp_request_lease(physical: Str) [fs, process, time, error] -> Result[DhcpLease] {
  var mac = []

  for iface in linux.interfaces()? {
    if iface.name == physical {
      mac = parse_mac(iface.mac)
    }
  }

  if mac.len() != 6 {
    return Err(IfupError.State(f"${physical}: could not read MAC address for DHCP"))
  }

  linux.link_up(physical)?
  let xid = time.now() % 4294967296
  let none = []
  let fd = linux.dhcp_socket(physical)?
  defer linux.dhcp_close(fd)?
  var offer = empty_lease()
  var attempt = 0

  while attempt < DHCP_RETRIES and ! offer.valid {
    linux.dhcp_send(fd, dhcp_packet(DHCP_DISCOVER, xid, mac, none, none)?)?
    let reply = linux.dhcp_recv(fd, DHCP_TIMEOUT_MS)?

    if reply.len() > 0 {
      let parsed = parse_dhcp_reply(reply, xid)?

      if parsed.valid and parsed.message_type == DHCP_OFFER {
        offer = parsed
      }
    }

    attempt = attempt + 1
  }

  return Err(IfupError.State(f"${physical}: no DHCP offer received")) unless offer.valid

  var lease = empty_lease()
  attempt = 0

  while attempt < DHCP_RETRIES and ! lease.valid {
    linux.dhcp_send(fd, dhcp_packet(DHCP_REQUEST, xid, mac, offer.yiaddr, offer.server_id)?)?
    let reply = linux.dhcp_recv(fd, DHCP_TIMEOUT_MS)?

    if reply.len() > 0 {
      let parsed = parse_dhcp_reply(reply, xid)?

      if parsed.valid and parsed.message_type == DHCP_ACK {
        lease = parsed
      }
    }

    attempt = attempt + 1
  }

  if ! lease.valid {
    return Err(IfupError.State(f"${physical}: DHCP request was not acknowledged"))
  }

  lease
}

proc write_resolv_conf(servers: List[Str]) [fs, error] {
  return when servers.len() == 0

  var body = ""

  for server in servers {
    body = f"""${body}nameserver ${server}
"""
  }

  fs.write(/etc/resolv.conf, body)?
}

proc configure_dhcp(physical: Str) [fs, process, time, error] {
  let lease = dhcp_request_lease(physical)?
  let address = ints_to_ip(lease.yiaddr)

  if address == "" {
    return Err(IfupError.State(f"${physical}: DHCP lease had no address"))
  }

  let netmask = if lease.netmask == "" { "255.255.255.0" } else { lease.netmask }
  linux.set_ipv4_address(physical, address, netmask)?

  if lease.gateway != "" {
    linux.add_default_ipv4_route(lease.gateway, interface: physical)?
  }

  write_resolv_conf(lease.dns)?
}

proc configure_static(physical: Str, stanza: Interface) [process, error] {
  if stanza.address == "" or stanza.netmask == "" {
    return Err(IfupError.Config(f"${stanza.logical}: static inet stanza requires address and netmask"))
  }

  linux.link_up(physical)?
  linux.set_ipv4_address(physical, stanza.address, stanza.netmask)?

  if stanza.gateway != "" {
    linux.add_default_ipv4_route(stanza.gateway, interface: physical)?
  }
}

proc configure_interface(config: Config, state_path: Path, physical: Str, logical: Str) [fs, process, time, error] {
  return when state_path.exists()? and state_has_iface(state_path.read_text()?, physical)

  let stanza = find_stanza(config, logical)?

  if stanza.family != "inet" {
    return Err(IfupError.Config(f"${stanza.logical}: unsupported address family ${stanza.family}"))
  }

  for command in stanza.pre_up {
    run_hook(command, physical, stanza, "pre-up")?
  }

  run_parts(/etc/network/if-pre-up.d, physical, stanza, "pre-up")?

  match stanza.method {
    "loopback" | "manual" => linux.link_up(physical)?
    "static" => configure_static(physical, stanza)?
    "dhcp" => configure_dhcp(physical)?
    _ => return Err(IfupError.Config(f"${stanza.logical}: unsupported method ${stanza.method}"))
  }

  for command in stanza.up {
    run_hook(command, physical, stanza, "post-up")?
  }

  for command in stanza.post_up {
    run_hook(command, physical, stanza, "post-up")?
  }

  run_parts(/etc/network/if-up.d, physical, stanza, "post-up")?
  mark_configured(state_path, physical, stanza.logical)?
}

pure split_iface_arg(arg: Str) -> InterfaceSelection {
  let parts = arg.split("=", maxsplit: 1)

  return {physical: parts[0], logical: parts[1]} when parts.len() >= 2

  {physical: arg, logical: arg}
}

type IfupOptions = {all: Bool, operands: List[Str]}

proc main(...argv: List[Str]) [fs, process, env, time, error] {
  let opts: IfupOptions = cli.applet(
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
    return Err(IfupError.Usage("ifup: expected -a or interface name"))
  }

  let config = parse_interfaces_file(default_interfaces_path()?, empty_config())?
  let state_path = default_state_path()?

  if all {
    for name in config.auto {
      configure_interface(config, state_path, name, name)?
    }
  }

  for operand in operands {
    let selection = split_iface_arg(operand)
    configure_interface(config, state_path, selection.physical, selection.logical)?
  }
}
