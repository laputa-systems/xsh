#!/usr/local/bin/xsh --
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

# Minimal IPv4 DHCP client (RFC 2131), modeled on busybox udhcpc but pared down
# to the DISCOVER/OFFER/REQUEST/ACK handshake. The broadcast UDP socket is
# provided by the linux.dhcp_* primitives; everything else is plain byte work.
let DHCP_MAGIC = [99, 130, 83, 99]
let DHCP_DISCOVER = 1
let DHCP_OFFER = 2
let DHCP_REQUEST = 3
let DHCP_ACK = 5
let DHCP_HEADER_LEN = 240
let DHCP_RETRIES = 5
let DHCP_TIMEOUT_MS = 3000

pure empty_interface() -> Interface {
  let pre_up: List[Str] = []
  let up: List[Str] = []
  let post_up: List[Str] = []

  return {
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
  let auto: List[Str] = []
  let interfaces: List[Interface] = []
  return {auto, interfaces}
}

proc default_interfaces_path() [env] -> Result[Path] {
  let raw = match env("XSH_IFUP_INTERFACES") { Ok(path_value) => path_value, Err(_) => "/etc/network/interfaces" }
  return fp"${raw}"
}

proc default_state_path() [env] -> Result[Path] {
  let raw = match env("XSH_IFUP_STATE") { Ok(path_value) => path_value, Err(_) => "/run/network/ifstate" }
  return fp"${raw}"
}

pure first_word(line: Str) -> Str {
  let words = line.words()

  if words.len() == 0 {
    return ""
  }

  return words[0]
}

pure rest_after_word(line: Str) -> Str {
  let word = first_word(line)

  if word == "" {
    return ""
  }

  return (line.split("") |> drop(word.count_chars())).join("").trim()
}

pure add_unique(items: List[Str], item: Str) -> List[Str] {
  if item in items {
    return items
  }

  return items.push(item)
}

pure glob_match(pattern: Str, text: Str) -> Bool {
  if pattern == "*" {
    return true
  }

  if ! pattern.contains("*") {
    return pattern == text
  }

  let parts = pattern.split("*")

  if pattern.starts_with("*") and pattern.ends_with("*") {
    return text.contains(parts[1])
  }

  if pattern.starts_with("*") {
    return text.ends_with(parts[1])
  }

  if pattern.ends_with("*") {
    return text.starts_with(parts[0])
  }

  return text.starts_with(parts[0]) and text.ends_with(parts[1])
}

pure append_current(config: Config, current: Interface) -> Config {
  if current.logical == "" {
    return config
  }

  return {...config, interfaces: config.interfaces.push(current)}
}

proc parse_source_path(source: Str, config: Config) [fs, error] -> Result[Config] {
  let path_value = fp"${source}"

  if ! source.contains("*") {
    return parse_interfaces_file(path_value, config)?
  }

  let dir = path_value.parent()
  let pattern = path_value.name()
  var result = config

  if ! dir.exists()? {
    return result
  }

  for entry in fs.children(dir)?
    |> where .kind == "file" and glob_match(pattern, .name)
    |> sort-by .name {
    result = parse_interfaces_file(entry.path, result)?
  }

  return result
}

proc parse_interfaces_file(path_value: Path, config: Config) [fs, error] -> Result[Config] {
  if ! path_value.exists()? {
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

  return append_current(result, current)
}

pure state_has_iface(state: Str, physical: Str) -> Bool {
  for line in state.lines() {
    let fields = line.words()

    if fields.len() >= 1 and fields[0].split("=")[0] == physical {
      return true
    }
  }

  return false
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

  if state_has_iface(text, physical) {
    return
  }

  if text != "" and ! text.ends_with("\n") {
    text = f"""${text}
"""
  }

  state_path.write_atomic(f"""${text}${physical}=${logical}
""")?
}

proc run_hook(command: Str, physical: Str, stanza: Interface, phase: Str) [process, error] {
  if command == "" {
    return
  }

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
  if ! dir.exists()? {
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
    if stanza.logical == logical {
      return stanza
    }
  }

  return Err(IfupError.Config(f"unknown interface ${logical}"))
}

pure hex_nibble(code: Int) -> Int {
  if code >= 48 and code <= 57 {
    return code - 48
  }

  if code >= 97 and code <= 102 {
    return code - 87
  }

  if code >= 65 and code <= 70 {
    return code - 55
  }

  return 0
}

pure parse_mac(mac: Str) -> List[Int] {
  [hex_nibble(part.byte_at(0)) * 16 + hex_nibble(part.byte_at(1)) for part in mac.split(":") if part != ""]
}

pure empty_lease() -> Record {
  let yiaddr: List[Int] = []
  let dns_servers: List[Str] = []
  let server_id: List[Int] = []

  return {
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
  if octets.len() != 4 {
    return ""
  }

  return f"${octets[0]}.${octets[1]}.${octets[2]}.${octets[3]}"
}

proc read_ip_octets(packet: Bytes, offset: Int) [error] -> Result[List[Int]] {
  return [
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
  var chunks: List[Bytes] = []
  chunks = chunks.push(bytes.from_ints([1, 1, 6, 0])?)
  chunks = chunks.push(bytes.pack_be(xid, 4)?)
  chunks = chunks.push(bytes.from_ints([0, 0])?)
  chunks = chunks.push(bytes.from_ints([128, 0])?)
  chunks = chunks.push(bytes.zero(16)?)
  chunks = chunks.push(bytes.from_ints(mac)?)
  chunks = chunks.push(bytes.zero(16 - mac.len())?)
  chunks = chunks.push(bytes.zero(192)?)
  chunks = chunks.push(bytes.from_ints(DHCP_MAGIC)?)
  var options: List[Int] = [53, 1, msg_type, 61, 7, 1].extend(mac)

  if requested_ip.len() == 4 {
    options = options.extend([50, 4]).extend(requested_ip)
  }

  if server_id.len() == 4 {
    options = options.extend([54, 4]).extend(server_id)
  }

  options = options.extend([55, 5, 1, 3, 6, 15, 28]).push(255)
  chunks = chunks.push(bytes.from_ints(options)?)
  return bytes.concat(chunks)
}

proc parse_dhcp_reply(packet: Bytes, xid: Int) [error] -> Result[Record] {
  let total = packet.len()

  if total < DHCP_HEADER_LEN {
    return empty_lease()
  }

  if bytes.unpack_be(packet, 1, 0)? != 2 or bytes.unpack_be(packet, 4, 4)? != xid {
    return empty_lease()
  }

  let yiaddr = read_ip_octets(packet, 16)?
  var message_type = 0
  var netmask = ""
  var gateway = ""
  var dns_servers: List[Str] = []
  var server_id: List[Int] = []
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

  return {
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
proc dhcp_request_lease(physical: Str) [fs, process, time, error] -> Result[Record] {
  var mac: List[Int] = []

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
  let none: List[Int] = []
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

  if ! offer.valid {
    return Err(IfupError.State(f"${physical}: no DHCP offer received"))
  }

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

  return lease
}

proc write_resolv_conf(servers: List[Str]) [fs, error] {
  if servers.len() == 0 {
    return
  }

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
  if state_path.exists()? and state_has_iface(state_path.read_text()?, physical) {
    return
  }

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

pure split_iface_arg(arg: Str) -> Record {
  let parts = arg.split("=")

  if parts.len() >= 2 {
    return {physical: parts[0], logical: parts[1]}
  }

  return {physical: arg, logical: arg}
}

proc main(...argv: List[Str]) [fs, process, env, time, error] {
  var all = false
  var operands: List[Str] = []

  for arg in argv {
    match arg {
      "-a" | "--all" => all = true
      "-v" | "--verbose" => {}
      _ => operands = operands.push(arg)
    }
  }

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

main(@args)?
