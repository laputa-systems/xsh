#!/bin/xsh
##! Query and change network device settings through the SIOCETHTOOL ioctl.
use lib.ethtool_decode as dec
use lib.gnu

const VERSION_LINE = "ethtool version 7.1"

# Byte lengths of the structures the ioctl returns whole: driver information,
# time stamping information, and the fixed head of the link settings.
const DRVINFO_SIZE = 196
const TS_INFO_SIZE = 44
const LINK_HEAD_SIZE = 48

# Commands in the order the help lists them. `names` are the spellings an
# invocation may use; `id` selects the implementation below.
type Verb = {id: Str, names: List[Str]}

const COMMANDS: List[Verb] = [
  {id: "change", names: ["-s", "--change"]},
  {id: "show-pause", names: ["-a", "--show-pause"]},
  {id: "pause", names: ["-A", "--pause"]},
  {id: "show-coalesce", names: ["-c", "--show-coalesce"]},
  {id: "coalesce", names: ["-C", "--coalesce"]},
  {id: "show-ring", names: ["-g", "--show-ring"]},
  {id: "set-ring", names: ["-G", "--set-ring"]},
  {id: "show-features", names: ["-k", "--show-features", "--show-offload"]},
  {id: "features", names: ["-K", "--features", "--offload"]},
  {id: "driver", names: ["-i", "--driver"]},
  {id: "test", names: ["-t", "--test"]},
  {id: "statistics", names: ["-S", "--statistics"]},
  {id: "show-time-stamping", names: ["-T", "--show-time-stamping"]},
  {id: "show-permaddr", names: ["-P", "--show-permaddr"]},
  {id: "show-channels", names: ["-l", "--show-channels"]},
  {id: "set-channels", names: ["-L", "--set-channels"]},
  {id: "show-eee", names: ["--show-eee"]},
  {id: "set-eee", names: ["--set-eee"]},
]

# Spellings ethtool has that need a driver-specific decoder, a netlink-only
# attribute, or a destructive operation. They fail by name instead of being
# treated as a device.
type Refusal = {names: List[Str], reason: Str}

const UNSUPPORTED: List[Refusal] = [
  {names: ["-d", "--register-dump"], reason: "register dumps need per-driver decoders"},
  {names: ["-e", "--eeprom-dump", "-E", "--change-eeprom"], reason: "EEPROM access is not implemented"},
  {names: ["-r", "--negotiate"], reason: "restarting negotiation is not implemented"},
  {names: ["-p", "--identify"], reason: "port identification blinks hardware and is not implemented"},
  {names: ["-n", "-u", "--show-nfc", "--show-ntuple", "-N", "-U", "--config-nfc", "--config-ntuple"], reason: "flow classification is not implemented"},
  {names: ["-x", "--show-rxfh-indir", "--show-rxfh", "-X", "--set-rxfh-indir", "--rxfh"], reason: "receive hash configuration is not implemented"},
  {names: ["-f", "--flash"], reason: "firmware flashing is not implemented"},
  {names: ["-w", "--get-dump", "-W", "--set-dump"], reason: "driver dumps are not implemented"},
  {names: ["-m", "--dump-module-eeprom", "--module-info"], reason: "module EEPROM decoding is not implemented"},
  {names: ["--show-priv-flags", "--set-priv-flags"], reason: "private flags are not implemented"},
  {names: ["--show-fec", "--set-fec"], reason: "FEC settings are not implemented"},
  {names: ["--phy-statistics", "--show-phys", "--cable-test", "--cable-test-tdr"], reason: "PHY operations are not implemented"},
  {names: ["--monitor", "-Q", "--per-queue"], reason: "kernel notifications and per-queue commands are not implemented"},
]

# Coalescing parameters that exist only as netlink attributes.
const COALESCE_UNAVAILABLE = ["cqe-mode-rx", "cqe-mode-tx", "tx-aggr-max-bytes", "tx-aggr-max-frames", "tx-aggr-time-usecs", "rx-cqe-frames", "rx-cqe-nsecs"]

const HELP = """
ethtool version 7.1
Usage:
        ethtool [ FLAGS ]  DEVNAME	Display standard information about device
        ethtool [ FLAGS ] -s|--change DEVNAME	Change generic options
		[ speed %d ]
		[ duplex half|full ]
		[ port tp|aui|bnc|mii|fibre|da ]
		[ mdix auto|on|off ]
		[ autoneg on|off ]
		[ advertise %x ]
		[ phyad %d ]
		[ wol p|u|m|b|a|g|s|f|d... ]
		[ sopass %x:%x:%x:%x:%x:%x ]
		[ msglvl %d[/%d] ]
        ethtool [ FLAGS ] -a|--show-pause DEVNAME	Show pause options
        ethtool [ FLAGS ] -A|--pause DEVNAME	Set pause options
		[ autoneg on|off ]
		[ rx on|off ]
		[ tx on|off ]
        ethtool [ FLAGS ] -c|--show-coalesce DEVNAME	Show coalesce options
        ethtool [ FLAGS ] -C|--coalesce DEVNAME	Set coalesce options
		[adaptive-rx on|off]
		[adaptive-tx on|off]
		[rx-usecs N]
		[rx-frames N]
		[rx-usecs-irq N]
		[rx-frames-irq N]
		[tx-usecs N]
		[tx-frames N]
		[tx-usecs-irq N]
		[tx-frames-irq N]
		[stats-block-usecs N]
		[pkt-rate-low N]
		[rx-usecs-low N]
		[rx-frames-low N]
		[tx-usecs-low N]
		[tx-frames-low N]
		[pkt-rate-high N]
		[rx-usecs-high N]
		[rx-frames-high N]
		[tx-usecs-high N]
		[tx-frames-high N]
		[sample-interval N]
        ethtool [ FLAGS ] -g|--show-ring DEVNAME	Query RX/TX ring parameters
        ethtool [ FLAGS ] -G|--set-ring DEVNAME	Set RX/TX ring parameters
		[ rx N ]
		[ rx-mini N ]
		[ rx-jumbo N ]
		[ tx N ]
        ethtool [ FLAGS ] -k|--show-features|--show-offload DEVNAME	Get state of protocol offload and other features
        ethtool [ FLAGS ] -K|--features|--offload DEVNAME	Set protocol offload and other features
		FEATURE on|off ...
        ethtool [ FLAGS ] -i|--driver DEVNAME	Show driver information
        ethtool [ FLAGS ] -t|--test DEVNAME	Execute adapter self test (refused)
        ethtool [ FLAGS ] -S|--statistics DEVNAME	Show adapter statistics
        ethtool [ FLAGS ] -T|--show-time-stamping DEVNAME	Show time stamping capabilities
        ethtool [ FLAGS ] -P|--show-permaddr DEVNAME	Show permanent hardware address
        ethtool [ FLAGS ] -l|--show-channels DEVNAME	Query Channels
        ethtool [ FLAGS ] -L|--set-channels DEVNAME	Set Channels
		[ rx N ]
		[ tx N ]
		[ other N ]
		[ combined N ]
        ethtool [ FLAGS ] --show-eee DEVNAME	Show EEE settings
        ethtool [ FLAGS ] --set-eee DEVNAME	Set EEE settings
		[ eee on|off ]
		[ advertise %x ]
		[ tx-lpi on|off ]
		[ tx-timer %d ]
        ethtool [ FLAGS ] -h|--help 		Show this help
        ethtool [ FLAGS ] --version 		Show version number

FLAGS:
	-j|--json	enable JSON output format (-k only)
"""

# Standard error is buffered by the runtime; flush it per message so a
# diagnostic is not reordered behind output written to standard output.
proc warn(text: Str) {
  let _ = io.write_stderr(text + "\n")
  let _ = io.flush_stderr()
}

proc bad_args() {
  warn("ethtool: bad command line argument(s)")
  warn("For more information run ethtool -h")
  exit 1
}

# One failed request: the kernel's reason in the netlink-style wording
# `ethtool` uses for the commands it serves over generic netlink. A missing
# device also gets the header-lookup line the kernel adds.
proc netlink_error(failure: Error) {
  if (failure.errno ?? 0) == 19 {
    warn("netlink error: no device matches name (offset 24)")
  }
  warn(f"netlink error: {gnu.strerror(failure)}")
}

proc netlink_fail(failure: Error, status: Int) {
  netlink_error(failure)
  exit status
}

proc emit(lines: List[Str]) {
  gnu.write_text(lines.join("\n") + "\n")
}

proc eth_request(fd: Int, device: Str, payload: Bytes, out_len: Int) -> Result[Bytes] {
  let c = linux.net_constants()
  let name = bytes.from_text(device.byte_slice(0, 15))
  let padded = bytes.concat([name, bytes.zero(16 - name.len())?])
  Ok(linux.ioctl(fd, c.SIOCETHTOOL, bytes.concat([padded, payload]), out_len)?)
}

# Reads the unsigned decimal value a parameter takes.
proc number_value(option: Str, name: Str, text: Str) -> Int {
  if let Ok(value) = text.parse_int() {
    return value when value >= 0
  }
  warn(f"ethtool ({option}): invalid value '{text}' for parameter '{name}'")
  exit 1
}

proc switch_value(option: Str, name: Str, text: Str) -> Bool {
  return true when text == "on"
  return false when text == "off"

  warn(f"ethtool ({option}): invalid value '{text}' for parameter '{name}'")
  exit 1
}

type Pair = {name: Str, value: Str}

# Splits `NAME VALUE ...` parameters, rejecting a name outside `known` and a
# name without a value, in command-line order.
proc pairs(option: Str, rest: List[Str], known: List[Str]) -> List[Pair] {
  var found: List[Pair] = []
  var index = 0
  while index < rest.len() {
    let name = rest[index]
    if name not in known {
      warn(f"ethtool ({option}): unknown parameter '{name}'")
      exit 1
    }
    if index + 1 >= rest.len() {
      warn(f"ethtool ({option}): no value for parameter '{name}'")
      exit 1
    }
    found += [{name: name, value: rest[index + 1]}]
    index += 2
  }
  found
}

# A show command takes the device and nothing else.
proc no_parameters(rest: List[Str]) {
  guard rest.is_empty() else {
    warn(f"ethtool: unexpected parameter '{rest[0]}'")
    exit 1
  }
}

proc show_driver(fd: Int, device: Str, rest: List[Str]) {
  bad_args() when ! rest.is_empty()

  match eth_request(fd, device, dec.drvinfo_request()?, DRVINFO_SIZE) {
    Ok(reply) => emit(dec.drvinfo_lines(dec.drvinfo_decode(reply)?))
    Err(failure) => {
      warn(f"Cannot get driver information: {gnu.strerror(failure)}")
      exit 71
    }
  }
}

type FeatureRead = {names: List[Str], features: List[dec.Feature]}

# Feature names and states through the string-set and feature ioctls.
proc read_features(fd: Int, device: Str, status: Int) -> Result[FeatureRead] {
  let sized = eth_request(fd, device, dec.sset_request(dec.SS_FEATURES)?, 20)
  if let Err(failure) = sized { netlink_fail(failure, status) }
  let count = dec.sset_length(sized?, dec.SS_FEATURES)?
  if count == 0 {
    warn("netlink error: no device features reported")
    exit status
  }
  let strings = eth_request(fd, device, dec.strings_request(dec.SS_FEATURES, count)?, 12 + count * 32)
  if let Err(failure) = strings { netlink_fail(failure, status) }
  let names = dec.string_set(strings?, count)?
  let blocks = (count + 31) / 32
  let state = eth_request(fd, device, dec.features_request(count)?, 8 + blocks * 16)
  if let Err(failure) = state { netlink_fail(failure, status) }
  Ok({names: names, features: dec.features_decode(names, state?)?})
}

proc show_features(fd: Int, device: Str, rest: List[Str], as_json: Bool) {
  no_parameters(rest)
  let read = read_features(fd, device, 1)?
  if as_json {
    gnu.write_text(dec.features_json(device, read.features) + "\n")
  } else {
    emit(dec.features_lines(device, read.features))
  }
}

# `-K`: `FEATURE on|off` pairs, up to a `--` that ends the list. The kernel
# rejects a whole request that names an unknown bit, at the byte offset of that
# bit within its request message; the offset is rebuilt here from the same
# attribute layout so the report names the same position.
proc set_features(fd: Int, device: Str, rest: List[Str]) {
  var words: List[Str] = []
  var values: List[Bool] = []
  var index = 0
  while index < rest.len() and rest[index] != "--" {
    let value = if index + 1 < rest.len() { rest[index + 1] } else { "" }
    if index + 1 >= rest.len() or value not in ["on", "off"] {
      let shown = if index + 1 < rest.len() { rest[index + 1] } else { "(null)" }
      warn(f"ethtool (-K): flag '{shown}' for parameter '(null)' is not followed by 'on' or 'off'")
      exit 1
    }
    words += [rest[index]]
    values += [value == "on"]
    index += 2
  }
  let read = read_features(fd, device, 92)?
  var wanted: Map[Int, Bool] = {}
  var offset = 48 + (device.byte_len() + 4) / 4 * 4
  for position in range(words.len()) {
    let named = dec.feature_names(words[position], read.names)
    if named.is_empty() and ! dec.is_flag(words[position]) {
      warn(f"netlink error: bit name not found (offset {offset})")
      warn(f"netlink error: {not_supported_text()?}")
      exit 92
    }
    for name in named {
      var at = 0
      for candidate in read.features {
        at = candidate.index when candidate.name == name
      }
      wanted = wanted.set(at, values[position])
      offset += 8 + (name.byte_len() + 4) / 4 * 4 + (if values[position] { 4 } else { 0 })
    }
  }
  if wanted.is_empty() {
    warn("Could not change any device features")
    exit 1
  }
  let request = dec.features_set_request(read.names.len(), wanted)?
  if let Err(failure) = eth_request(fd, device, request, request.len()) { netlink_fail(failure, 92) }
  let blocks = (read.names.len() + 31) / 32
  let after = eth_request(fd, device, dec.features_request(read.names.len())?, 8 + blocks * 16)
  if let Err(failure) = after { netlink_fail(failure, 92) }
  let outcome = dec.features_outcome(read.features, dec.features_decode(read.names, after?)?, wanted)
  if ! outcome.lines.is_empty() {
    gnu.write_text("Actual changes:\n")
  }
  if ! outcome.changed and outcome.unsatisfied {
    warn("Could not change any device features")
    if ! outcome.lines.is_empty() { emit(outcome.lines) }
    exit 1
  }
  if ! outcome.lines.is_empty() { emit(outcome.lines) }
}

# The libc wording for EOPNOTSUPP, taken from a request the kernel refuses for
# exactly that reason: a datagram socket cannot listen.
proc not_supported_text() -> Result[Str] {
  let c = linux.net_constants()
  let probe = linux.socket(c.AF_INET, c.SOCK_DGRAM)?
  defer unix.close_fd(probe)
  if let Err(failure) = linux.listen(probe) { return Ok(gnu.strerror(failure)) }
  Ok("Operation not supported")
}

proc show_statistics(fd: Int, device: Str, rest: List[Str]) {
  for word in rest {
    if word in ["--all-groups", "--groups", "--src"] {
      warn(f"ethtool (-S): parameter '{word}' is not supported")
      exit 1
    }
    warn(f"ethtool (-S): unknown parameter '{word}'")
    exit 1
  }
  let sized = eth_request(fd, device, dec.sset_request(dec.SS_STATS)?, 20)
  if let Err(failure) = sized {
    warn(f"Cannot get stats strings information: {gnu.strerror(failure)}")
    exit 96
  }
  let count = dec.sset_length(sized?, dec.SS_STATS)?
  if count == 0 {
    warn("no stats available")
    exit 94
  }
  let strings = eth_request(fd, device, dec.strings_request(dec.SS_STATS, count)?, 12 + count * 32)
  if let Err(failure) = strings {
    warn(f"Cannot get stats strings information: {gnu.strerror(failure)}")
    exit 96
  }
  let values = eth_request(fd, device, dec.stats_request(count)?, 8 + count * 8)
  if let Err(failure) = values {
    warn(f"Cannot get stats information: {gnu.strerror(failure)}")
    exit 97
  }
  emit(dec.stats_lines(dec.string_set(strings?, count)?, values?)?)
}

proc show_time_stamping(fd: Int, device: Str, rest: List[Str]) {
  for word in rest {
    if word in ["index", "qualifier"] {
      warn(f"ethtool (-T): parameter '{word}' is not supported")
      exit 1
    }
    warn(f"ethtool (-T): unknown parameter '{word}'")
    exit 1
  }
  match eth_request(fd, device, dec.tsinfo_request()?, TS_INFO_SIZE) {
    Ok(reply) => emit(dec.tsinfo_lines(device, reply)?)
    Err(failure) => netlink_fail(failure, 1)
  }
}

proc show_permaddr(fd: Int, device: Str, rest: List[Str]) {
  no_parameters(rest)
  let request = bytes.concat([bytes.pack_le(dec.CMD.GPERMADDR, 4)?, bytes.pack_le(32, 4)?, bytes.zero(32)?])
  match eth_request(fd, device, request, 40) {
    Ok(reply) => gnu.write_text(dec.permaddr_line(reply)? + "\n")
    Err(failure) => {
      warn(f"netlink error: {gnu.strerror(failure)}")
      exit 1
    }
  }
}

proc show_channels(fd: Int, device: Str, rest: List[Str]) {
  no_parameters(rest)
  match eth_request(fd, device, dec.words(dec.CMD.GCHANNELS, [0, 0, 0, 0, 0, 0, 0, 0])?, 36) {
    Ok(reply) => emit(dec.channels_lines(device, reply)?)
    Err(failure) => netlink_fail(failure, 1)
  }
}

proc set_channels(fd: Int, device: Str, rest: List[Str]) {
  let names = ["rx", "tx", "other", "combined"]
  let given = pairs("-L", rest, names)
  return when given.is_empty()

  var requested: List[Int?] = [null, null, null, null]
  for pair in given {
    for position in range(4) {
      requested[position] = number_value("-L", pair.name, pair.value) when names[position] == pair.name
    }
  }
  let current = eth_request(fd, device, dec.words(dec.CMD.GCHANNELS, [0, 0, 0, 0, 0, 0, 0, 0])?, 36)
  if let Err(failure) = current { netlink_fail(failure, 1) }
  let request = dec.channels_set_request(current?, requested)?
  if let Err(failure) = eth_request(fd, device, request, 36) {
    if (failure.errno ?? 0) == 22 {
      let reason = dec.channels_refusal(current?, requested, device.byte_len())?
      if reason != null { warn(f"netlink error: {reason}") }
    }
    netlink_fail(failure, 1)
  }
}

proc show_ring(fd: Int, device: Str, rest: List[Str]) {
  no_parameters(rest)
  match eth_request(fd, device, dec.words(dec.CMD.GRINGPARAM, [0, 0, 0, 0, 0, 0, 0, 0])?, 36) {
    Ok(reply) => emit(dec.ring_lines(device, reply)?)
    Err(failure) => netlink_fail(failure, 1)
  }
}

proc set_ring(fd: Int, device: Str, rest: List[Str]) {
  let names = ["rx", "rx-mini", "rx-jumbo", "tx"]
  for word in rest {
    if word in ["rx-buf-len", "tcp-data-split", "cqe-size", "tx-push", "rx-push", "tx-push-buf-len", "hds-thresh"] {
      warn(f"ethtool (-G): parameter '{word}' is not available through the ioctl interface")
      exit 1
    }
  }
  let given = pairs("-G", rest, names)
  var requested: List[Int?] = [null, null, null, null]
  for pair in given {
    for position in range(4) {
      requested[position] = number_value("-G", pair.name, pair.value) when names[position] == pair.name
    }
  }
  let current = eth_request(fd, device, dec.words(dec.CMD.GRINGPARAM, [0, 0, 0, 0, 0, 0, 0, 0])?, 36)
  if let Err(failure) = current { netlink_fail(failure, 81) }
  return when given.is_empty()

  let request = dec.ring_set_request(current?, requested)?
  if let Err(failure) = eth_request(fd, device, request, 36) { netlink_fail(failure, 81) }
}

proc show_coalesce(fd: Int, device: Str, rest: List[Str]) {
  no_parameters(rest)
  match eth_request(fd, device, dec.words(dec.CMD.GCOALESCE, range_zero(22))?, 92) {
    Ok(reply) => emit(dec.coalesce_lines(device, reply)?)
    Err(failure) => netlink_fail(failure, 1)
  }
}

pure range_zero(count: Int) -> List[Int] {
  var zeros: List[Int] = []
  for _ in range(count) { zeros += [0] }
  zeros
}

proc set_coalesce(fd: Int, device: Str, rest: List[Str]) {
  for word in rest {
    if word in COALESCE_UNAVAILABLE {
      warn(f"ethtool (-C): parameter '{word}' is not available through the ioctl interface")
      exit 1
    }
  }
  let given = pairs("-C", rest, dec.COALESCE_FIELDS)
  var changes: Map[Int, Int] = {}
  for pair in given {
    for position in range(dec.COALESCE_FIELDS.len()) {
      continue when dec.COALESCE_FIELDS[position] != pair.name

      let adaptive = pair.name in ["adaptive-rx", "adaptive-tx"]
      let value = if adaptive { if switch_value("-C", pair.name, pair.value) { 1 } else { 0 } } else { number_value("-C", pair.name, pair.value) }
      changes = changes.set(position, value)
    }
  }
  let current = eth_request(fd, device, dec.words(dec.CMD.GCOALESCE, range_zero(22))?, 92)
  if let Err(failure) = current { netlink_fail(failure, 1) }
  return when given.is_empty()

  let request = dec.coalesce_set_request(current?, changes)?
  if let Err(failure) = eth_request(fd, device, request, 92) { netlink_fail(failure, 1) }
}

proc show_pause(fd: Int, device: Str, rest: List[Str]) {
  for word in rest {
    if word == "--src" {
      warn("ethtool (-a): parameter '--src' is not available through the ioctl interface")
      exit 1
    }
    warn(f"ethtool (-a): unknown parameter '{word}'")
    exit 1
  }
  match eth_request(fd, device, dec.words(dec.CMD.GPAUSEPARAM, [0, 0, 0])?, 16) {
    Ok(reply) => emit(dec.pause_lines(device, reply)?)
    Err(failure) => netlink_fail(failure, 1)
  }
}

proc set_pause(fd: Int, device: Str, rest: List[Str]) {
  var autoneg: Bool? = null
  var rx: Bool? = null
  var tx: Bool? = null
  for pair in pairs("-A", rest, ["autoneg", "rx", "tx"]) {
    let value = switch_value("-A", pair.name, pair.value)
    if pair.name == "autoneg" { autoneg = value } else if pair.name == "rx" { rx = value } else { tx = value }
  }
  let current = eth_request(fd, device, dec.words(dec.CMD.GPAUSEPARAM, [0, 0, 0])?, 16)
  if let Err(failure) = current { netlink_fail(failure, 76) }
  let request = dec.pause_set_request(current?, autoneg, rx, tx)?
  if let Err(failure) = eth_request(fd, device, request, 16) { netlink_fail(failure, 76) }
}

proc show_eee(fd: Int, device: Str, rest: List[Str]) {
  no_parameters(rest)
  match eth_request(fd, device, dec.words(dec.CMD.GEEE, range_zero(9))?, 40) {
    Ok(reply) => emit(dec.eee_lines(device, reply)?)
    Err(failure) => netlink_fail(failure, 1)
  }
}

proc set_eee(fd: Int, device: Str, rest: List[Str]) {
  var enabled: Bool? = null
  var tx_lpi: Bool? = null
  var advertise: Int? = null
  var timer: Int? = null
  for pair in pairs("--set-eee", rest, ["eee", "advertise", "tx-lpi", "tx-timer"]) {
    if pair.name == "eee" {
      enabled = switch_value("--set-eee", pair.name, pair.value)
    } else if pair.name == "tx-lpi" {
      tx_lpi = switch_value("--set-eee", pair.name, pair.value)
    } else if pair.name == "advertise" {
      advertise = number_value("--set-eee", pair.name, pair.value)
    } else {
      timer = number_value("--set-eee", pair.name, pair.value)
    }
  }
  let current = eth_request(fd, device, dec.words(dec.CMD.GEEE, range_zero(9))?, 40)
  if let Err(failure) = current { netlink_fail(failure, 76) }
  let request = dec.eee_set_request(current?, advertise, enabled, tx_lpi, timer)?
  if let Err(failure) = eth_request(fd, device, request, 40) { netlink_fail(failure, 76) }
}

# Self-tests can interrupt traffic and offline tests take the link down, so
# a device that offers any is refused; one that has none gets the kernel's own
# answer to a test request.
proc run_test(fd: Int, device: Str, rest: List[Str]) {
  bad_args() when rest.len() > 1 or (rest.len() == 1 and rest[0] not in ["online", "offline", "external_lb"])

  let sized = eth_request(fd, device, dec.sset_request(0)?, 20)
  if let Err(failure) = sized {
    warn(f"Cannot get strings: {gnu.strerror(failure)}")
    exit 74
  }
  let count = dec.sset_length(sized?, 0)?
  if count > 0 {
    warn(f"ethtool: self-test refused: {device} offers {count} tests that can interrupt traffic")
    exit 74
  }
  let request = dec.words(26, [0, 0, 0])?
  if let Err(failure) = eth_request(fd, device, request, 16) {
    warn(f"Cannot test: {gnu.strerror(failure)}")
    exit 74
  }
  warn("Cannot test: no self-tests")
  exit 74
}

# What the device reports for `ethtool DEV`: link settings, wake-on-LAN, the
# message level, and link state, each independently available.
proc show_settings(fd: Int, device: Str) {
  var lines: List[Str] = []
  var any = false
  let first = eth_request(fd, device, dec.link_settings_request(0)?, LINK_HEAD_SIZE)
  if let Err(failure) = first {
    if (failure.errno ?? 0) == 19 {
      repeat 7 times { netlink_error(failure) }
      gnu.write_text("No data available\n")
      exit 75
    }
    if (failure.errno ?? 0) != 95 { netlink_error(failure) }
  } else {
    let wanted = -dec.link_settings_nwords(first?)
    let full = eth_request(fd, device, dec.link_settings_request(wanted)?, LINK_HEAD_SIZE + wanted * 12)
    if let Err(failure) = full {
      netlink_error(failure)
    } else {
      lines += dec.settings_lines(dec.link_settings_decode(full?)?)
      any = true
    }
  }
  let wol = eth_request(fd, device, dec.words(dec.CMD.GWOL, [0, 0, 0, 0])?, 20)
  if let Err(failure) = wol {
    netlink_error(failure) when (failure.errno ?? 0) != 95
  } else {
    lines += dec.wol_lines(wol?)?
    any = true
  }
  let level = eth_request(fd, device, dec.words(dec.CMD.GMSGLVL, [0])?, 8)
  if let Err(failure) = level {
    netlink_error(failure) when (failure.errno ?? 0) != 95
  } else {
    lines += dec.msglvl_lines(bytes.unpack_le(level?, 4, 4)?)
    any = true
  }
  let link = eth_request(fd, device, dec.words(dec.CMD.GLINK, [0])?, 8)
  if let Err(failure) = link {
    netlink_error(failure) when (failure.errno ?? 0) != 95
  } else {
    lines += [f"\tLink detected: {if bytes.unpack_le(link?, 4, 4)? != 0 { "yes" } else { "no" }}"]
    any = true
  }
  if ! any {
    gnu.write_text("No data available\n")
    exit 75
  }
  emit([f"Settings for {device}:"] + lines)
}

# `-s`: the generic settings of the link. Fields are written back into the
# settings the kernel reported, so everything not named keeps its value.
proc change_settings(fd: Int, device: Str, rest: List[Str]) {
  let known = ["speed", "duplex", "port", "mdix", "autoneg", "advertise", "phyad", "wol", "sopass", "msglvl"]
  for word in rest {
    if word in ["lanes", "xcvr", "mode", "master-slave", "type"] {
      warn(f"ethtool (-s): parameter '{word}' is not available through the ioctl interface")
      exit 1
    }
  }
  let given = pairs("-s", rest, known)
  return when given.is_empty()

  var speed: Int? = null
  var duplex: Int? = null
  var port: Int? = null
  var phy: Int? = null
  var autoneg: Int? = null
  var mdix: Int? = null
  var advertise: Int? = null
  var wol: Int? = null
  var secure: Bytes? = null
  var level: Int? = null
  var level_mask: Int? = null
  for pair in given {
    if pair.name == "speed" {
      speed = number_value("-s", "speed", pair.value)
    } else if pair.name == "duplex" {
      duplex = choice("-s", "duplex", pair.value, ["half", "full"])
    } else if pair.name == "port" {
      port = choice("-s", "port", pair.value, ["tp", "aui", "mii", "fibre", "bnc", "da"])
    } else if pair.name == "mdix" {
      mdix = 1 + choice("-s", "mdix", pair.value, ["off", "on", "auto"])
    } else if pair.name == "autoneg" {
      autoneg = if switch_value("-s", "autoneg", pair.value) { 1 } else { 0 }
    } else if pair.name == "advertise" {
      advertise = number_value("-s", "advertise", pair.value)
    } else if pair.name == "phyad" {
      phy = number_value("-s", "phyad", pair.value)
    } else if pair.name == "wol" {
      let mask = dec.wol_parse(pair.value)
      if mask == null {
        warn(f"ethtool (-s): invalid value '{pair.value}' for parameter 'wol'")
        exit 1
      }
      wol = mask
    } else if pair.name == "sopass" {
      let octets = pair.value.split(":")
      var password: List[Int] = []
      for octet in octets { password += [("0x" + octet).parse_int() ?? -1] }
      if octets.len() != 6 or ! [o for o in password if o < 0 or o > 255].is_empty() {
        warn(f"ethtool (-s): invalid value '{pair.value}' for parameter 'sopass'")
        exit 1
      }
      secure = bytes.from_ints(password)?
    } else {
      let parts = pair.value.split("/")
      level = number_value("-s", "msglvl", parts[0])
      level_mask = if parts.len() > 1 { number_value("-s", "msglvl", parts[1]) } else { null }
    }
  }
  let link_changes = speed != null or duplex != null or port != null or phy != null or autoneg != null or mdix != null or advertise != null
  if link_changes {
    apply_link_settings(fd, device, speed, duplex, port, phy, autoneg, mdix, advertise)
  }
  if wol != null or secure != null {
    let current = eth_request(fd, device, dec.words(dec.CMD.GWOL, [0, 0, 0, 0])?, 20)
    if let Err(failure) = current { netlink_fail(failure, 75) }
    let before = current?
    let options = wol ?? bytes.unpack_le(before, 4, 8)?
    let password = secure ?? before.slice(12, 6)
    let request = bytes.concat([bytes.pack_le(dec.CMD.SWOL, 4)?, bytes.pack_le(bytes.unpack_le(before, 4, 4)?, 4)?, bytes.pack_le(options, 4)?, password, bytes.zero(2)?])
    if let Err(failure) = eth_request(fd, device, request, 20) { netlink_fail(failure, 75) }
  }
  if level != null {
    let current = eth_request(fd, device, dec.words(dec.CMD.GMSGLVL, [0])?, 8)
    if let Err(failure) = current { netlink_fail(failure, 75) }
    let before = bytes.unpack_le(current?, 4, 4)?
    var next = level ?? 0
    if level_mask != null {
      let keep = before - before.bit_and(level_mask)
      next = keep + (level ?? 0).bit_and(level_mask)
    }
    if let Err(failure) = eth_request(fd, device, dec.words(dec.CMD.SMSGLVL, [next])?, 8) { netlink_fail(failure, 75) }
  }
}

# The index of `text` among `choices`, or a usage failure naming the value.
proc choice(option: Str, name: Str, text: Str, choices: List[Str]) -> Int {
  for index in range(choices.len()) {
    return index when choices[index] == text
  }
  warn(f"ethtool ({option}): invalid value '{text}' for parameter '{name}'")
  exit 1
}

proc apply_link_settings(fd: Int, device: Str, speed: Int?, duplex: Int?, port: Int?, phy: Int?, autoneg: Int?, mdix: Int?, advertise: Int?) {
  let first = eth_request(fd, device, dec.link_settings_request(0)?, LINK_HEAD_SIZE)
  if let Err(failure) = first {
    warn("netlink error: failed to retrieve link settings") when autoneg != null
    netlink_fail(failure, 75)
  }
  let wanted = -dec.link_settings_nwords(first?)
  let full = eth_request(fd, device, dec.link_settings_request(wanted)?, LINK_HEAD_SIZE + wanted * 12)
  if let Err(failure) = full {
    warn("netlink error: failed to retrieve link settings") when autoneg != null
    netlink_fail(failure, 75)
  }
  let settings = dec.link_settings_decode(full?)?
  # Autonegotiation advertises either the given modes or, when the request
  # turns it on or changes speed or duplex under it, every supported mode.
  let final_autoneg = autoneg ?? settings.autoneg
  var advertising = settings.advertising
  if advertise != null {
    advertising = [advertise]
    repeat settings.nwords - 1 times { advertising += [0] }
  } else if final_autoneg == 1 and (autoneg != null or speed != null or duplex != null) {
    advertising = settings.supported
  }
  let request = dec.link_settings_set_request(settings, speed, duplex, port, phy, autoneg, mdix, advertising)?
  if let Err(failure) = eth_request(fd, device, request, request.len()) { netlink_fail(failure, 75) }
}

proc main(...argv: List[Str]) {
  var args = argv
  var as_json = false
  while ! args.is_empty() {
    let word = args[0]
    if word in ["-j", "--json"] {
      as_json = true
    } else if word in ["--debug", "--disable-netlink", "-I", "--include-statistics"] {
      warn(f"ethtool: {word} is not supported: this implementation has one ioctl-backed code path and no debug trace")
      exit 1
    } else {
      break
    }
    args = args[1..]
  }
  bad_args() when args.is_empty()

  let word = args[0]
  if word in ["-h", "--help"] {
    gnu.help(HELP)
    return
  }
  if word == "--version" {
    gnu.write_text(VERSION_LINE + "\n")
    return
  }
  var command = "settings"
  var rest: List[Str] = args[1..]
  if word.starts_with("-") {
    command = ""
    for entry in COMMANDS {
      command = entry.id when word in entry.names
    }
    if command == "" {
      for entry in UNSUPPORTED {
        if word in entry.names {
          warn(f"ethtool: {word} is not supported: {entry.reason}")
          exit 1
        }
      }
      bad_args()
    }
    bad_args() when rest.is_empty()

    args = rest
    rest = rest[1..]
  }
  let device = args[0]
  if as_json and command != "show-features" {
    warn("ethtool: bad command line argument(s)")
    warn("JSON output not available for this subcommand")
    warn("For more information run ethtool -h")
    exit 1
  }
  let c = linux.net_constants()
  let fd = linux.socket(c.AF_INET, c.SOCK_DGRAM)
  if let Err(failure) = fd {
    gnu.error(f"cannot open control socket: {gnu.strerror(failure)}")
    exit 1
  }
  let socket = fd?
  defer unix.close_fd(socket)
  let outcome = match command {
    "settings" => {
      no_parameters(rest)
      show_settings(socket, device)
    }
    "driver" => show_driver(socket, device, rest)
    "show-features" => show_features(socket, device, rest, as_json)
    "features" => set_features(socket, device, rest)
    "statistics" => show_statistics(socket, device, rest)
    "show-time-stamping" => show_time_stamping(socket, device, rest)
    "show-permaddr" => show_permaddr(socket, device, rest)
    "show-channels" => show_channels(socket, device, rest)
    "set-channels" => set_channels(socket, device, rest)
    "show-ring" => show_ring(socket, device, rest)
    "set-ring" => set_ring(socket, device, rest)
    "show-coalesce" => show_coalesce(socket, device, rest)
    "coalesce" => set_coalesce(socket, device, rest)
    "show-pause" => show_pause(socket, device, rest)
    "pause" => set_pause(socket, device, rest)
    "show-eee" => show_eee(socket, device, rest)
    "set-eee" => set_eee(socket, device, rest)
    "test" => run_test(socket, device, rest)
    else => change_settings(socket, device, rest)
  }
  if let Err(failure) = outcome {
    gnu.error(failure.message)
    exit 1
  }
}
