#!/bin/xsh
# iw: show and change nl80211 wireless devices through generic netlink.
#
# Commands: dev, dev DEV info|link|scan|station|set type|channel|freq|txpower,
# phy, list, phy PHY info|reg get|set txpower|channel|freq, reg get|set, event,
# help, and --version. Message layouts, decoders, and report formats live in
# `lib/nl80211.xsh`; this file owns the command line, usage text, and the
# order of requests. Association, authentication, mesh, AP, and the other
# `iw` commands are absent and are not listed in the help.
use lib.gnu
use lib.nl80211 as nl

const VERSION = "6.17"

# `key` names the command for dispatch. `section` is the word before the
# name (`reg` in `reg get`), `idby` is `dev`, `phy`, or empty, and `args` is
# the usage text after the name.
type Entry = {key: Str, section: Str, name: Str, idby: Str, args: Str, help: List[Str]}

const SCAN_ARGS = "[-u] [freq <freq>*] [duration <dur>] [ies <hex as 00:11:..>] [meshid <meshid>] [lowpri,flush,ap-force,duration-mandatory] [ssid <ssid>*|passive]"
const TRIGGER_ARGS = "[freq <freq>*] [duration <dur>] [ies <hex as 00:11:..>] [meshid <meshid>] [lowpri,flush,ap-force,duration-mandatory,coloc] [ssid <ssid>*|passive]"
const CHANNEL_ARGS = "<channel> [NOHT|HT20|HT40+|HT40-]"
const FREQ_ARGS = "<freq> [NOHT|HT20|HT40+|HT40-]"
const TXPOWER_ARGS = "<auto|fixed|limit> [<tx power in mBm>]"

# The commands in the order `iw help` lists them. A command with no help text
# prints only its usage line, as `iw` does for the `phy` alias of `list`.
const COMMANDS: List[Entry] = [
  {
    key: "event",
    section: "",
    name: "event",
    idby: "",
    args: "[-t|-T|-r]",
    help: [
      "Monitor events from the kernel.",
      "-t - print timestamp",
      "-T - print absolute, human-readable timestamp",
      "-r - print relative timestamp",
    ],
  },
  {key: "list", section: "", name: "phy", idby: "", args: "", help: []},
  {
    key: "list",
    section: "",
    name: "list",
    idby: "",
    args: "",
    help: ["List all wireless devices and their capabilities."],
  },
  {
    key: "phy.info",
    section: "",
    name: "info",
    idby: "phy",
    args: "",
    help: ["Show capabilities for the specified wireless device."],
  },
  {key: "dev", section: "", name: "dev", idby: "", args: "", help: ["List all network interfaces for wireless hardware."]},
  {key: "dev.info", section: "", name: "info", idby: "dev", args: "", help: ["Show information for this interface."]},
  {
    key: "help",
    section: "",
    name: "help",
    idby: "",
    args: "[command]",
    help: ["Print usage for all or a specific command, e.g.", "\"help wowlan\" or \"help wowlan enable\"."],
  },
  {
    key: "dev.link",
    section: "",
    name: "link",
    idby: "dev",
    args: "",
    help: ["Print information about the current connection, if any."],
  },
  {
    key: "reg.reload",
    section: "reg",
    name: "reload",
    idby: "",
    args: "",
    help: ["Reload the kernel's regulatory database."],
  },
  {
    key: "phy.reg.get",
    section: "reg",
    name: "get",
    idby: "phy",
    args: "",
    help: ["Print out the devices' current regulatory domain information."],
  },
  {
    key: "reg.get",
    section: "reg",
    name: "get",
    idby: "",
    args: "",
    help: ["Print out the kernel's current regulatory domain information."],
  },
  {
    key: "reg.set",
    section: "reg",
    name: "set",
    idby: "",
    args: "<ISO/IEC 3166-1 alpha2>",
    help: ["Notify the kernel about the current regulatory domain."],
  },
  {
    key: "scan",
    section: "scan",
    name: "",
    idby: "dev",
    args: SCAN_ARGS,
    help: [
      "Scan on the given frequencies and probe for the given SSIDs",
      "(or wildcard if not given) unless passive scanning is requested.",
      "If -u is specified print unknown data in the scan results.",
      "Specified (vendor) IEs must be well-formed.",
    ],
  },
  {key: "scan.abort", section: "scan", name: "abort", idby: "dev", args: "", help: ["Abort ongoing scan"]},
  {
    key: "scan.trigger",
    section: "scan",
    name: "trigger",
    idby: "dev",
    args: TRIGGER_ARGS,
    help: [
      "Trigger a scan on the given frequencies with probing for the given",
      "SSIDs (or wildcard if not given) unless passive scanning is requested.",
      "Duration(in TUs), if specified, will be used to set dwell times.",
      "",
    ],
  },
  {
    key: "scan.dump",
    section: "scan",
    name: "dump",
    idby: "dev",
    args: "[-u]",
    help: ["Dump the current scan results. If -u is specified, print unknown", "data in scan results."],
  },
  {
    key: "set.type",
    section: "set",
    name: "type",
    idby: "dev",
    args: "<type>",
    help: ["Set interface type/mode.", "Valid interface types are: managed, ibss, monitor, mesh, wds."],
  },
  {
    key: "set.txpower",
    section: "set",
    name: "txpower",
    idby: "dev",
    args: TXPOWER_ARGS,
    help: ["Specify transmit power level and setting type."],
  },
  {
    key: "set.txpower",
    section: "set",
    name: "txpower",
    idby: "phy",
    args: TXPOWER_ARGS,
    help: ["Specify transmit power level and setting type."],
  },
  {key: "set.channel", section: "set", name: "channel", idby: "dev", args: CHANNEL_ARGS, help: []},
  {key: "set.channel", section: "set", name: "channel", idby: "phy", args: CHANNEL_ARGS, help: []},
  {key: "set.freq", section: "set", name: "freq", idby: "dev", args: FREQ_ARGS, help: []},
  {
    key: "set.freq",
    section: "set",
    name: "freq",
    idby: "phy",
    args: FREQ_ARGS,
    help: ["Set frequency/channel configuration the hardware is using."],
  },
  {
    key: "station.dump",
    section: "station",
    name: "dump",
    idby: "dev",
    args: "",
    help: ["List all stations known, e.g. the AP on managed interfaces"],
  },
  {
    key: "station.get",
    section: "station",
    name: "get",
    idby: "dev",
    args: "<MAC address>",
    help: ["Get information for a specific station."],
  },
]

const FOOTER = """

Commands that use the netdev ('dev') can also be given the
'wdev' instead to identify the device.

You can omit the 'phy' or 'dev' if the identification is unique,
e.g. "iw wlan0 info" or "iw phy0 info". (Don't when scripting.)

Do NOT screenscrape this tool, we don't consider its output stable.
"""

# The words that name the command in a usage line, with the device
# placeholder where the command takes one.
pure usage_words(command: Entry) -> Str {
  var words: List[Str] = []
  if command.idby == "dev" {
    words += ["dev <devname>"]
  } else if command.idby == "phy" {
    words += ["phy <phyname>"]
  }
  if command.section != "" {
    words += [command.section]
  }
  if command.name != "" {
    words += [command.name]
  }
  if command.args != "" {
    words += [command.args]
  }
  # `iw` prints the empty argument list of `scan abort` after a space.
  if command.key == "scan.abort" {
    words += [""]
  }
  words.join(" ")
}

# The commands `iw help WORDS` selects: a section command matches its section
# word and then its name, and a plain command matches its name.
pure selected(command: Entry, words: List[Str]) -> Bool {
  if words.is_empty() {
    return true
  }
  if command.section != "" {
    if words[0] != command.section {
      return false
    }
    if words.len() == 1 {
      return true
    }
    return command.name != "" and words[1] == command.name
  }
  command.name == words[0]
}

pure usage_header() -> Str {
  "Usage:\tiw [options] command\nOptions:\n\t--version\tshow version ({VERSION})\nCommands:\n".replace("{VERSION}", with: VERSION)
}

# Full help text for `words`, or the bare command list when `brief`.
pure help_text(words: List[Str], brief: Bool) -> Str {
  var text = usage_header()
  for command in COMMANDS {
    if ! selected(command, words) {
      continue
    }
    text += f"\t{usage_words(command)}\n"
    if ! brief and ! command.help.is_empty() {
      for line in command.help {
        text += f"\t\t{line}\n"
      }
      text += "\n"
    }
  }
  # `iw` ends its usage with an empty line after the closing remark.
  text + FOOTER + "\n\n"
}

# The usage of one command, as `iw` prints it when its arguments are wrong.
pure command_usage(key: Str, idby: Str) -> Str {
  for command in COMMANDS {
    if command.key == key and command.idby == idby {
      var text = f"Usage:\tiw [options] {usage_words(command)}\n"
      if ! command.help.is_empty() {
        text += "\n" + command.help.join("\n") + "\n"
      }
      return text
    }
  }
  help_text([], false)
}

# Writes `lines` to stdout, flushing so events appear as they arrive.
proc emit(lines: List[Str]) {
  if ! lines.is_empty() {
    gnu.write_text(lines.join("\n") + "\n")
  }
}

# --- device selection --------------------------------------------------

# A resolved `dev` or `phy` identifier: the ifindex or wiphy index and the
# name to print back.
type Target = {kind: Str, index: Int, name: Str}

proc resolve(client: nl.Client, kind: Str, name: Str) -> Result[Target?] {
  if kind == "phy" {
    if name.starts_with("phy#") {
      let number = name.byte_slice(4).parse_int()
      if let Ok(index) = number {
        return Ok({kind: kind, index: index, name: name})
      }
      return Err(nl.errno_failure(2))
    }
    let found = nl.find_phy(client, name)?
    if found == null {
      return Ok(null)
    }
    return Ok({kind: kind, index: found, name: name})
  }
  let found = nl.find_interface(client, name)?
  if found == null {
    return Ok(null)
  }
  Ok({kind: kind, index: found.index, name: name})
}

# --- commands ----------------------------------------------------------

proc command_dev(client: nl.Client) -> Result[Int] {
  let messages = nl.request(client, nl.CMD.get_interface, [], dump: true)?
  emit(nl.dev_lines(messages))
  Ok(0)
}

proc command_list(client: nl.Client, filter: List[Bytes]) -> Result[Int] {
  let messages = nl.request(client, nl.CMD.get_wiphy, [nl.attr_flag(nl.ATTRS.split)?] + filter, dump: true)?
  emit(nl.phy_lines(messages))
  Ok(0)
}

proc command_info(client: nl.Client, target: Target) -> Result[Int] {
  let messages = nl.request(client, nl.CMD.get_interface, [nl.attr_u32(nl.ATTRS.ifindex, target.index)?])?
  if messages.is_empty() {
    return Err(nl.errno_failure(19))
  }
  emit(nl.info_lines(messages[0]))
  Ok(0)
}

proc command_link(client: nl.Client, target: Target) -> Result[Int] {
  let scans = nl.request(client, nl.CMD.get_scan, [nl.attr_u32(nl.ATTRS.ifindex, target.index)?], dump: true)?
  let connection = nl.connection_of(scans, target.name)
  if connection == null {
    emit(["Not connected."])
    return Ok(0)
  }
  emit(connection.lines)
  let stations = nl.request(
    client,
    nl.CMD.get_station,
    [nl.attr_u32(nl.ATTRS.ifindex, target.index)?, nl.attr(nl.ATTRS.mac, connection.bssid)?],
  )?
  if ! stations.is_empty() {
    emit(nl.link_lines(stations[0]))
  }
  Ok(0)
}

# Milliseconds since boot, whole seconds only, which is the clock `iw` dates
# an association against; 0 when the host cannot say.
proc boot_millis() -> Int {
  (unix.uptime_seconds() ?? 0) * 1000
}

proc station_report(messages: List[nl.Message], name: Str) {
  let boot = boot_millis()
  let wall = time.now() / 1000000000 * 1000
  for message in messages {
    emit(nl.station_lines(message, name, boot, wall))
  }
}

proc command_station_dump(client: nl.Client, target: Target) -> Result[Int] {
  let messages = nl.request(client, nl.CMD.get_station, [nl.attr_u32(nl.ATTRS.ifindex, target.index)?], dump: true)?
  station_report(messages, target.name)
  Ok(0)
}

proc command_station_get(client: nl.Client, target: Target, mac: Bytes) -> Result[Int] {
  let attrs = [nl.attr_u32(nl.ATTRS.ifindex, target.index)?, nl.attr(nl.ATTRS.mac, mac)?]
  let messages = nl.request(client, nl.CMD.get_station, attrs)?
  station_report(messages, target.name)
  Ok(0)
}

proc command_scan_dump(client: nl.Client, target: Target, unknown: Bool) -> Result[Int] {
  let messages = nl.request(client, nl.CMD.get_scan, [nl.attr_u32(nl.ATTRS.ifindex, target.index)?], dump: true)?
  for message in messages {
    emit(nl.bss_lines(message, target.name, unknown))
  }
  Ok(0)
}

# Triggers a scan. With `follow`, blocks until the kernel reports the scan
# finished or aborted on this interface and then dumps the results.
proc command_scan(client: nl.Client, target: Target, request: nl.ScanRequest, unknown: Bool, follow: Bool) -> Result[Int] {
  if follow {
    let groups = nl.multicast_groups(client)?
    guard let scan_group = groups.get("scan") else {
      return Err(nl.errno_failure(2))
    }
    nl.subscribe(client, scan_group)?
  }
  let _ = nl.request(client, nl.CMD.trigger_scan, nl.scan_attrs(target.index, request)?)?
  if ! follow {
    return Ok(0)
  }
  while true {
    let event = nl.receive_event(client)?
    if event == null {
      continue
    }
    let index = nl.pick_uint(event.attrs, nl.ATTRS.ifindex)
    if index != null and index != target.index {
      continue
    }
    if event.cmd == nl.CMD.new_scan_results {
      return command_scan_dump(client, target, unknown)
    }
    if event.cmd == nl.CMD.scan_aborted {
      emit(["scan aborted!"])
      return Ok(0)
    }
  }
  Ok(0)
}

proc command_scan_abort(client: nl.Client, target: Target) -> Result[Int] {
  let _ = nl.request(client, nl.CMD.abort_scan, [nl.attr_u32(nl.ATTRS.ifindex, target.index)?])?
  Ok(0)
}

proc command_reg_get(client: nl.Client, phy: Int?) -> Result[Int] {
  if phy == null {
    let messages = nl.request(client, nl.CMD.get_reg, [], dump: true)?
    emit(nl.reg_lines(messages))
  } else {
    let messages = nl.request(client, nl.CMD.get_reg, [nl.attr_u32(nl.ATTRS.wiphy, phy)?])?
    emit(nl.reg_lines(messages))
  }
  Ok(0)
}

proc command_reg_set(client: nl.Client, alpha2: Str) -> Result[Int] {
  let _ = nl.request(client, nl.CMD.req_set_reg, [nl.attr_str(nl.ATTRS.reg_alpha2, alpha2)?])?
  Ok(0)
}

# The attribute that names the device of a SET_WIPHY request.
proc device_attr(target: Target) -> Result[Bytes] {
  if target.kind == "dev" {
    nl.attr_u32(nl.ATTRS.ifindex, target.index)
  } else {
    nl.attr_u32(nl.ATTRS.wiphy, target.index)
  }
}

proc command_set_type(client: nl.Client, target: Target, kind: Int) -> Result[Int] {
  let attrs = [nl.attr_u32(nl.ATTRS.ifindex, target.index)?, nl.attr_u32(nl.ATTRS.iftype, kind)?]
  let _ = nl.request(client, nl.CMD.set_interface, attrs)?
  Ok(0)
}

proc command_set_txpower(client: nl.Client, target: Target, setting: Int, level: Int?) -> Result[Int] {
  var attrs = [device_attr(target)?, nl.attr_u32(nl.ATTRS.tx_power_setting, setting)?]
  if level != null {
    attrs += [nl.attr_u32(nl.ATTRS.tx_power_level, level)?]
  }
  let _ = nl.request(client, nl.CMD.set_wiphy, attrs)?
  Ok(0)
}

# One SET_WIPHY request that selects a control frequency and channel width.
proc command_set_frequency(client: nl.Client, target: Target, freq: Int, mode: Str) -> Result[Int] {
  var width = nl.WIDTHS.noht
  var center = freq
  match mode {
    "HT20" => width = nl.WIDTHS.w20
    "HT40+" => {
      width = nl.WIDTHS.w40
      center = freq + 10
    }
    "HT40-" => {
      width = nl.WIDTHS.w40
      center = freq - 10
    }
    else => {}
  }
  let attrs = [
    device_attr(target)?,
    nl.attr_u32(nl.ATTRS.freq, freq)?,
    nl.attr_u32(nl.ATTRS.width, width)?,
    nl.attr_u32(nl.ATTRS.center1, center)?,
  ]
  let _ = nl.request(client, nl.CMD.set_wiphy, attrs)?
  Ok(0)
}

# Prints events as the kernel reports them until the socket fails. `stamp`
# is `-t` (epoch seconds), `-T` (calendar time), or `-r` (seconds since the
# previous event).
proc command_event(client: nl.Client, stamp: Str) -> Result[Int] {
  let groups = nl.multicast_groups(client)?
  for name in ["config", "scan", "regulatory", "mlme", "vendor", "nan"] {
    if let Ok(id) = groups.get(name) {
      nl.subscribe(client, id)?
    } else if name != "vendor" and name != "nan" {
      return Err(nl.errno_failure(2))
    }
  }
  var names: Map[Str] = {}
  for entry in nl.interfaces(client)? {
    names = names.set(f"{entry.index}", entry.name)
  }
  var previous = time.now()
  while true {
    let event = nl.receive_event(client)?
    if event == null {
      continue
    }
    let now = time.now()
    var prefix = ""
    if stamp == "-t" {
      prefix = f"{now / 1000000000}.{now / 1000 % 1000000:06}: "
    } else if stamp == "-T" {
      let calendar = time.format(now, "%Y-%m-%d %H:%M:%S")?
      prefix = f"[{calendar}.{now / 1000 % 1000000:06}]: "
    } else if stamp == "-r" {
      let delta = now - previous
      prefix = f"+{delta / 1000000000}.{delta / 1000 % 1000000:06}: "
    }
    previous = now
    emit([prefix + nl.event_line(event, names)])
  }
  Ok(0)
}

# --- command line ------------------------------------------------------

# Words that name a command or a device selector, which an implicit device
# name cannot be.
const RESERVED = ["dev", "phy", "wdev", "list", "event", "reg", "help", "features", "commands"]

pure scan_words() -> List[Str] {
  ["freq", "ies", "meshid", "duration", "ssid", "passive", "lowpri", "flush", "ap-force", "duration-mandatory", "coloc"]
}

# Hex bytes written as `00:11:..`, or null when malformed.
pure parse_hex_list(text: Str) -> Bytes? {
  var values: List[Int] = []
  for part in text.split(":") {
    if ! rx"^[0-9A-Fa-f]{1,2}$".matches(part) {
      return null
    }
    values += [("0x" + part).parse_int() ?? 0]
  }
  if let Ok(packed) = bytes.from_ints(values) {
    return packed
  }
  null
}

# The scan options of `iw scan` and `iw scan trigger`; null means the words
# are not valid and the usage is shown.
pure parse_scan(words: List[Str], trigger: Bool) -> nl.ScanRequest? {
  var ssids: List[Str] = []
  var freqs: List[Int] = []
  var elements = b""
  var duration: Int? = null
  var mandatory = false
  var flags = 0
  var passive = false
  var mode = "none"
  var index = 0
  while index < words.len() {
    let word = words[index]
    if mode == "freq" {
      let number = word.parse_int()
      if let Ok(freq) = number {
        freqs += [freq]
        index += 1
        continue
      }
      mode = "none"
    } else if mode == "ssid" {
      if ! (word in scan_words()) {
        ssids += [word]
        index += 1
        continue
      }
      mode = "none"
    }
    match word {
      "freq" => {
        mode = "freq"
      }
      "ssid" => {
        mode = "ssid"
      }
      "passive" => {
        passive = true
      }
      "lowpri" => {
        flags = flags.bit_or(nl.SCAN_FLAGS.lowpri)
      }
      "flush" => {
        flags = flags.bit_or(nl.SCAN_FLAGS.flush)
      }
      "ap-force" => {
        flags = flags.bit_or(nl.SCAN_FLAGS.ap_force)
      }
      "coloc" => {
        if ! trigger {
          return null
        }
        flags = flags.bit_or(nl.SCAN_FLAGS.coloc)
      }
      "duration-mandatory" => {
        mandatory = true
      }
      "duration" => {
        index += 1
        if index >= words.len() {
          return null
        }
        let value = words[index].parse_int()
        if let Ok(number) = value {
          duration = number
        } else {
          return null
        }
      }
      "ies" => {
        index += 1
        if index >= words.len() {
          return null
        }
        let parsed = parse_hex_list(words[index])
        if parsed == null {
          return null
        }
        elements = bytes.concat([elements, parsed])
      }
      "meshid" => {
        index += 1
        if index >= words.len() {
          return null
        }
        let id = bytes.from_text(words[index])
        if id.len() > 32 {
          return null
        }
        if let Ok(header) = bytes.from_ints([114, id.len()]) {
          elements = bytes.concat([elements, header, id])
        } else {
          return null
        }
      }
      else => {
        return null
      }
    }
    index += 1
  }
  if passive and ! ssids.is_empty() {
    return null
  }
  {ssids: ssids, freqs: freqs, elements: elements, duration: duration, mandatory: mandatory, flags: flags, passive: passive}
}

# What a device-scoped command line means: the command key and its words
# after the key, or an empty key for an unknown command.
type Identified = {key: Str, rest: List[Str]}

pure identify(idby: Str, words: List[Str]) -> Identified {
  let none = {key: "", rest: []}
  if words.is_empty() {
    return none
  }
  if idby == "phy" {
    if words[0] == "info" {
      return {key: "phy.info", rest: words[1..]}
    }
    if words.len() >= 2 and words[0] == "reg" and words[1] == "get" {
      return {key: "phy.reg.get", rest: words[2..]}
    }
    if words.len() >= 2 and words[0] == "set" {
      match words[1] {
        "txpower" => return {key: "set.txpower", rest: words[2..]}
        "channel" => return {key: "set.channel", rest: words[2..]}
        "freq" => return {key: "set.freq", rest: words[2..]}
        else => return none
      }
    }
    return none
  }
  match words[0] {
    "info" => return {key: "dev.info", rest: words[1..]}
    "link" => return {key: "dev.link", rest: words[1..]}
    "scan" => {
      if words.len() >= 2 {
        match words[1] {
          "dump" => return {key: "scan.dump", rest: words[2..]}
          "trigger" => return {key: "scan.trigger", rest: words[2..]}
          "abort" => return {key: "scan.abort", rest: words[2..]}
          else => {}
        }
      }
      return {key: "scan", rest: words[1..]}
    }
    "station" => {
      if words.len() >= 2 {
        match words[1] {
          "dump" => return {key: "station.dump", rest: words[2..]}
          "get" => return {key: "station.get", rest: words[2..]}
          else => {}
        }
      }
      return none
    }
    "set" => {
      if words.len() >= 2 {
        match words[1] {
          "type" => return {key: "set.type", rest: words[2..]}
          "txpower" => return {key: "set.txpower", rest: words[2..]}
          "channel" => return {key: "set.channel", rest: words[2..]}
          "freq" => return {key: "set.freq", rest: words[2..]}
          else => {}
        }
      }
      return none
    }
    else => return none
  }
}

# Parses a trailing `[NOHT|HT20|HT40+|HT40-]` after a channel or frequency;
# null when the word is not a mode.
pure ht_mode(rest: List[Str]) -> Str? {
  if rest.is_empty() {
    return "NOHT"
  }
  if rest.len() > 1 {
    return null
  }
  match rest[0] {
    "NOHT" | "HT20" | "HT40+" | "HT40-" => rest[0]
    else => null
  }
}

# Runs one device-scoped command. `1` is returned for arguments that do not
# fit the command, which the caller reports with that command's usage.
proc run_device_command(client: nl.Client, target: Target, key: Str, rest: List[Str]) -> Result[Int] {
  match key {
    "phy.info" => {
      if ! rest.is_empty() {
        return Ok(1)
      }
      return command_list(client, [nl.attr_u32(nl.ATTRS.wiphy, target.index)?])
    }
    "phy.reg.get" => {
      if ! rest.is_empty() {
        return Ok(1)
      }
      return command_reg_get(client, target.index)
    }
    "dev.info" => {
      if ! rest.is_empty() {
        return Ok(1)
      }
      return command_info(client, target)
    }
    "dev.link" => {
      if ! rest.is_empty() {
        return Ok(1)
      }
      return command_link(client, target)
    }
    "station.dump" => {
      if ! rest.is_empty() {
        return Ok(1)
      }
      return command_station_dump(client, target)
    }
    "station.get" => {
      if rest.len() != 1 {
        return Ok(1)
      }
      let mac = nl.parse_mac(rest[0])
      if mac == null {
        eprint "invalid mac address"
        return Ok(2)
      }
      return command_station_get(client, target, mac)
    }
    "scan.dump" => {
      if rest.len() > 1 or (rest.len() == 1 and rest[0] != "-u") {
        return Ok(1)
      }
      return command_scan_dump(client, target, ! rest.is_empty())
    }
    "scan.abort" => {
      if ! rest.is_empty() {
        return Ok(1)
      }
      return command_scan_abort(client, target)
    }
    "scan.trigger" => {
      let request = parse_scan(rest, true)
      if request == null {
        return Ok(1)
      }
      return command_scan(client, target, request, false, false)
    }
    "scan" => {
      var words = rest
      var unknown = false
      if ! words.is_empty() and words[0] == "-u" {
        unknown = true
        words = words[1..]
      }
      let request = parse_scan(words, false)
      if request == null {
        return Ok(1)
      }
      return command_scan(client, target, request, unknown, true)
    }
    "set.type" => {
      if rest.len() != 1 {
        return Ok(1)
      }
      let kind = nl.parse_iftype(rest[0])
      if kind == null {
        eprint f"invalid interface type {rest[0]}"
        return Ok(2)
      }
      return command_set_type(client, target, kind)
    }
    "set.txpower" => {
      if rest.is_empty() {
        return Ok(1)
      }
      match rest[0] {
        "auto" => {
          if rest.len() != 1 {
            return Ok(1)
          }
          return command_set_txpower(client, target, nl.TX_POWER.auto, null)
        }
        "fixed" | "limit" => {
          if rest.len() < 2 {
            eprint "Missing TX power level argument."
            return Ok(2)
          }
          if rest.len() != 2 {
            return Ok(1)
          }
          let level = rest[1].parse_int()
          if let Ok(mbm) = level {
            let setting = if rest[0] == "fixed" { nl.TX_POWER.fixed } else { nl.TX_POWER.limit }
            return command_set_txpower(client, target, setting, mbm)
          }
          return Ok(1)
        }
        else => {
          eprint f"Invalid parameter: {rest[0]}"
          return Ok(2)
        }
      }
    }
    "set.channel" | "set.freq" => {
      if rest.is_empty() {
        return Ok(1)
      }
      let number = rest[0].parse_int()
      guard let value = number else {
        return Ok(1)
      }
      let mode = ht_mode(rest[1..])
      if mode == null {
        return Ok(1)
      }
      var freq = value
      if key == "set.channel" {
        let converted = nl.channel_frequency(value)
        if converted == null {
          return Ok(1)
        }
        freq = converted
      }
      return command_set_frequency(client, target, freq, mode)
    }
    else => return Ok(1)
  }
}

# Reports a failed command the way `iw` does and returns its exit status:
# the negated errno modulo 256 for a kernel failure.
proc report(failure: Error) -> Int {
  match failure {
    nl.IwError.Errno {code, text} => {
      eprint f"command failed: {text} (-{code})"
      (256 - code) % 256
    }
    nl.IwError.Missing => {
      eprint "nl80211 not found."
      1
    }
    nl.IwError.NoSocket => {
      eprint "Failed to connect to generic netlink."
      1
    }
    else => {
      eprint f"iw: {failure.message}"
      1
    }
  }
}

proc show_usage(key: Str, idby: Str, status: Int) -> Int {
  gnu.write_text(command_usage(key, idby))
  status
}

proc execute(tokens: List[Str]) -> Result[Int] {
  if tokens.is_empty() {
    gnu.write_text(help_text([], true))
    return Ok(0)
  }
  let first = tokens[0]
  if first == "--version" {
    gnu.write_text(f"iw version {VERSION}\n")
    return Ok(0)
  }
  if first == "help" {
    gnu.write_text(help_text(tokens[1..], false))
    return Ok(0)
  }
  # Which selector the line begins with, and the words that remain.
  var idby = ""
  var name = ""
  var words: List[Str] = []
  if first == "dev" or first == "phy" or first == "wdev" {
    if tokens.len() == 1 {
      if first == "dev" {
        let client = nl.connect()?
        defer nl.disconnect(client)
        return command_dev(client)
      }
      if first == "phy" {
        let client = nl.connect()?
        defer nl.disconnect(client)
        return command_list(client, [])
      }
      return Ok(show_usage("", "", 1))
    }
    if tokens.len() == 2 or first == "wdev" {
      return Ok(show_usage("", "", 1))
    }
    idby = first
    name = tokens[1]
    words = tokens[2..]
  } else if first == "list" {
    if tokens.len() != 1 {
      return Ok(show_usage("", "", 1))
    }
    let client = nl.connect()?
    defer nl.disconnect(client)
    return command_list(client, [])
  } else if first == "reg" {
    if tokens.len() == 2 and tokens[1] == "get" {
      let client = nl.connect()?
      defer nl.disconnect(client)
      return command_reg_get(client, null)
    }
    if tokens.len() == 2 and tokens[1] == "reload" {
      let client = nl.connect()?
      defer nl.disconnect(client)
      let _ = nl.request(client, nl.CMD.reload_regdb, [])?
      return Ok(0)
    }
    if tokens.len() == 3 and tokens[1] == "set" {
      if tokens[2].byte_len() != 2 {
        eprint "not a valid ISO/IEC 3166-1 alpha2\nSpecial non-alpha2 usable entries:\n\t00\tWorld Regulatory domain"
        return Ok(2)
      }
      let client = nl.connect()?
      defer nl.disconnect(client)
      return command_reg_set(client, tokens[2])
    }
    if tokens.len() >= 2 and tokens[1] == "set" {
      return Ok(show_usage("reg.set", "", 1))
    }
    return Ok(show_usage("", "", 1))
  } else if first == "event" {
    var stamp = ""
    if tokens.len() > 2 or (tokens.len() == 2 and tokens[1] != "-t" and tokens[1] != "-T" and tokens[1] != "-r") {
      return Ok(show_usage("event", "", 1))
    }
    if tokens.len() == 2 {
      stamp = tokens[1]
    }
    let client = nl.connect()?
    defer nl.disconnect(client)
    return command_event(client, stamp)
  } else if ! (first in RESERVED) and ! first.starts_with("-") and tokens.len() >= 2 {
    name = first
    words = tokens[1..]
    idby = "implicit"
  } else {
    return Ok(show_usage("", "", 1))
  }

  let client = nl.connect()?
  defer nl.disconnect(client)
  var selector = idby
  var target = null
  if idby == "implicit" {
    target = resolve(client, "dev", name)?
    selector = "dev"
    if target == null {
      target = resolve(client, "phy", name)?
      selector = "phy"
    }
    if target == null {
      return Ok(show_usage("", "", 1))
    }
  } else {
    target = resolve(client, selector, name)?
    if target == null {
      return Err(nl.errno_failure(if selector == "dev" { 19 } else { 2 }))
    }
  }
  let identified = identify(selector, words)
  if identified.key == "" {
    return Ok(show_usage("", "", 1))
  }
  let status = run_device_command(client, target, identified.key, identified.rest)?
  if status == 1 {
    return Ok(show_usage(identified.key, selector, 1))
  }
  Ok(status)
}

proc main(...argv: List[Str]) {
  let outcome = execute(argv)
  match outcome {
    Ok(status) => {
      if status != 0 {
        exit status
      }
    }
    Err(failure) => exit report(failure)
  }
}
